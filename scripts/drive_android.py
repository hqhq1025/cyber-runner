#!/usr/bin/env python3
"""
Cyber Runner Android driver — 北邮西土城路线版

读 ~/Documents/Github/run/more_route/xitucheng/xitucheng_1.txt（BD-09 坐标）
→ 转 WGS-84
→ 每 1 秒走一步（按速度计算位移，默认 4.7 m/s 配速 3:33/km）
→ 通过 adb shell am broadcast 喂给手机 FakeGPS 的 manifest receiver

用法：
  ./drive_android.py                    # 默认 4.7 m/s
  ./drive_android.py 5.5                # 指定 m/s
  ./drive_android.py 4.7 -s SERIAL      # 指定设备
"""

import argparse
import math
import subprocess
import sys
import time

ADB = "/Users/haoqing/Library/Android/sdk/platform-tools/adb"
ROUTE_FILE = "/Users/haoqing/Documents/Github/run/more_route/xitucheng/xitucheng_1.txt"
PACKAGE = "com.langqi.fakegps"
ACTION = "com.langqi.fakegps.MOCK_LOCATION"
DT = 1.0  # 秒/点

# ---------- BD-09 → WGS-84 (与 run.py 一致) ----------
X_PI = math.pi * 3000.0 / 180.0
A = 6378245.0
EE = 0.006693421622965943


def _transform_lat(x, y):
    ret = -100.0 + 2.0 * x + 3.0 * y + 0.2 * y * y + 0.1 * x * y + 0.2 * math.sqrt(abs(x))
    ret += (20.0 * math.sin(6.0 * x * math.pi) + 20.0 * math.sin(2.0 * x * math.pi)) * 2.0 / 3.0
    ret += (20.0 * math.sin(y * math.pi) + 40.0 * math.sin(y / 3.0 * math.pi)) * 2.0 / 3.0
    ret += (160.0 * math.sin(y / 12.0 * math.pi) + 320 * math.sin(y * math.pi / 30.0)) * 2.0 / 3.0
    return ret


def _transform_lon(x, y):
    ret = 300.0 + x + 2.0 * y + 0.1 * x * x + 0.1 * x * y + 0.1 * math.sqrt(abs(x))
    ret += (20.0 * math.sin(6.0 * x * math.pi) + 20.0 * math.sin(2.0 * x * math.pi)) * 2.0 / 3.0
    ret += (20.0 * math.sin(x * math.pi) + 40.0 * math.sin(x / 3.0 * math.pi)) * 2.0 / 3.0
    ret += (150.0 * math.sin(x / 12.0 * math.pi) + 300.0 * math.sin(x / 30.0 * math.pi)) * 2.0 / 3.0
    return ret


def bd09_to_wgs84(lng, lat):
    x, y = lng - 0.0065, lat - 0.006
    z = math.sqrt(x * x + y * y) - 0.00002 * math.sin(y * X_PI)
    theta = math.atan2(y, x) - 0.000003 * math.cos(x * X_PI)
    gcj_lng = z * math.cos(theta)
    gcj_lat = z * math.sin(theta)
    d_lat = _transform_lat(gcj_lng - 105.0, gcj_lat - 35.0)
    d_lng = _transform_lon(gcj_lng - 105.0, gcj_lat - 35.0)
    rad_lat = gcj_lat / 180.0 * math.pi
    magic = 1 - EE * math.sin(rad_lat) ** 2
    sqrt_magic = math.sqrt(magic)
    d_lng = (d_lng * 180.0) / (A / sqrt_magic * math.cos(rad_lat) * math.pi)
    d_lat = (d_lat * 180.0) / (A * (1 - EE) / (magic * sqrt_magic) * math.pi)
    return gcj_lng - d_lng, gcj_lat - d_lat


# ---------- 距离 (Haversine, 米) ----------
def haversine(lat1, lng1, lat2, lng2):
    R = 6371008.8
    phi1, phi2 = math.radians(lat1), math.radians(lat2)
    dphi = math.radians(lat2 - lat1)
    dlam = math.radians(lng2 - lng1)
    a = math.sin(dphi / 2) ** 2 + math.cos(phi1) * math.cos(phi2) * math.sin(dlam / 2) ** 2
    return 2 * R * math.atan2(math.sqrt(a), math.sqrt(1 - a))


# ---------- 路线加载 ----------
def load_route(path):
    import ast
    with open(path) as f:
        raw = ast.literal_eval("[" + f.read().strip().rstrip(",") + "]")
    pts = []
    for p in raw:
        bd_lng = float(p["lng"])
        bd_lat = float(p["lat"])
        w_lng, w_lat = bd09_to_wgs84(bd_lng, bd_lat)
        pts.append((w_lat, w_lng))
    return pts


# ---------- 按等步长重采样（v m/s, dt s）----------
def resample(route, v, dt):
    step = v * dt  # 米/步
    out = []
    for i in range(len(route)):
        a_lat, a_lng = route[i]
        b_lat, b_lng = route[(i + 1) % len(route)]
        seg = haversine(a_lat, a_lng, b_lat, b_lng)
        n = max(1, int(seg / step))
        for j in range(n):
            t = j / n
            out.append((a_lat + t * (b_lat - a_lat), a_lng + t * (b_lng - a_lng)))
    return out


# ---------- adb 投递 ----------
def adb_broadcast(serial, lat, lng, speed, acc=4.0):
    cmd = [
        ADB, "-s", serial, "shell", "am", "broadcast",
        "-a", ACTION,
        "-p", PACKAGE,
        "--es", "lat", f"{lat:.7f}",
        "--es", "lng", f"{lng:.7f}",
        "--es", "speed", f"{speed:.2f}",
        "--es", "acc", f"{acc:.1f}",
    ]
    subprocess.run(cmd, capture_output=True, check=False)


def adb_wake_activity(serial):
    """把 FakeGPS activity 唤到前台，脱离 OPPO 的 cached 状态。"""
    subprocess.run(
        [ADB, "-s", serial, "shell", "am", "start", "-n",
         f"{PACKAGE}/.FakeGPSActivity"],
        capture_output=True, check=False,
    )


def list_devices():
    out = subprocess.run([ADB, "devices"], capture_output=True, text=True)
    serials = []
    for line in out.stdout.splitlines()[1:]:
        parts = line.split()
        if len(parts) >= 2 and parts[1] == "device":
            serials.append(parts[0])
    return serials


# ---------- main loop ----------
def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("speed", nargs="?", type=float, default=4.7,
                    help="速度 m/s（默认 4.7 = 配速 3:33/km）")
    ap.add_argument("-s", "--serial", help="adb 设备序列号")
    ap.add_argument("--once", action="store_true", help="跑一圈就停")
    args = ap.parse_args()

    serials = list_devices()
    if not serials:
        print("❌ 没有 adb device，确认 USB 调试已开 + 已授权")
        sys.exit(1)
    serial = args.serial or serials[0]
    print(f"📱 设备: {serial}")

    print(f"📂 加载路线: {ROUTE_FILE}")
    route = load_route(ROUTE_FILE)
    print(f"   原始点数: {len(route)}")

    points = resample(route, args.speed, DT)
    total_m = sum(
        haversine(points[i][0], points[i][1], points[i + 1][0], points[i + 1][1])
        for i in range(len(points) - 1)
    )
    print(f"🏃 速度: {args.speed} m/s, 配速: "
          f"{int(1000 / args.speed // 60):02d}:{int(1000 / args.speed % 60):02d} /km")
    print(f"   重采样后: {len(points)} 点，每秒 1 点，单圈 {total_m:.0f} m, "
          f"耗时 {len(points)} s")

    print("🔄 唤醒 FakeGPS activity ...")
    adb_wake_activity(serial)
    time.sleep(2)

    print(f"🚀 开始喂坐标，Ctrl+C 退出\n")
    lap = 0
    i = 0
    started = time.time()
    try:
        while True:
            lat, lng = points[i]
            adb_broadcast(serial, lat, lng, args.speed)
            elapsed = int(time.time() - started)
            sys.stdout.write(
                f"\r🌍 lap{lap} {i+1:4d}/{len(points)} "
                f"({lat:.5f},{lng:.5f})  elapsed={elapsed//60:02d}:{elapsed%60:02d}"
            )
            sys.stdout.flush()
            i += 1
            if i >= len(points):
                lap += 1
                i = 0
                if args.once:
                    break
                # 每圈重新唤醒一次，防 OPPO 把 activity 切 cached
                adb_wake_activity(serial)
            time.sleep(DT)
    except KeyboardInterrupt:
        print("\n\n👋 停了。GPS 还停在最后一个点（如要恢复，重启手机或停 FakeGPS 服务）")


if __name__ == "__main__":
    main()
