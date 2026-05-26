#!/usr/bin/env python3
"""
Cyber Runner — iOS GPS location simulator via pymobiledevice3.

Reads route from RUN_ROUTE_CONFIG (relative to this script's dir),
converts BD-09 to WGS-84, resamples at RUN_SPEED m/s, and feeds
coordinates to a connected iOS device via pymobiledevice3.

Environment variables:
  RUN_ROUTE_CONFIG  — relative path to route file (e.g. "more_route/xitucheng/xitucheng_1.txt")
  RUN_SPEED         — speed in m/s (default 4.7)
"""

import json
import math
import os
import signal
import sys
import time

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
DEFAULT_SPEED = 4.7
DT = 1.0  # seconds per point

# ---------- BD-09 to WGS-84 (matches Rust coord.rs golden values) ----------
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
    ret += (150.0 * math.sin(x / 12.0 * math.pi) + 300 * math.sin(x * math.pi / 30.0)) * 2.0 / 3.0
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


def haversine(lat1, lng1, lat2, lng2):
    R = 6371008.8
    phi1, phi2 = math.radians(lat1), math.radians(lat2)
    dphi = math.radians(lat2 - lat1)
    dlam = math.radians(lng2 - lng1)
    a = math.sin(dphi / 2) ** 2 + math.cos(phi1) * math.cos(phi2) * math.sin(dlam / 2) ** 2
    return 2 * R * math.atan2(math.sqrt(a), math.sqrt(1 - a))


# ---------- Route loading ----------
def load_route(path):
    with open(path, encoding="utf-8") as f:
        raw_text = f.read().strip().rstrip(",")
    wrapped = "[" + raw_text + "]"
    raw = json.loads(wrapped)
    pts = []
    for p in raw:
        bd_lng = float(p["lng"])
        bd_lat = float(p["lat"])
        w_lng, w_lat = bd09_to_wgs84(bd_lng, bd_lat)
        pts.append((w_lat, w_lng))
    return pts


# ---------- Resample route at equal step length ----------
def resample(route, v, dt):
    step = v * dt
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


# ---------- pymobiledevice3 persistent connection ----------
def create_dvt_service():
    """Create a persistent DVT service connection to the iOS device."""
    from pymobiledevice3.lockdown import create_using_usbmux
    from pymobiledevice3.services.dvt.dvt_secure_socket_proxy import DvtSecureSocketProxyService

    print("connecting to device...")
    lockdown = create_using_usbmux()
    print(f"connected to: { lockdown.product_type if hasattr(lockdown, 'product_type') else 'iOS device' }")
    dvt = DvtSecureSocketProxyService(lockdown=lockdown)
    return dvt


def set_location(dvt, lat, lng):
    """Set simulated GPS location using persistent DVT connection."""
    dvt.simulate_location(lat, lng)


def clear_location(dvt):
    """Clear simulated GPS location."""
    try:
        dvt.clear_location()
    except Exception:
        pass


# ---------- Main ----------
def main():
    route_config = os.environ.get("RUN_ROUTE_CONFIG", "more_route/xitucheng/xitucheng_1.txt")
    speed = float(os.environ.get("RUN_SPEED", DEFAULT_SPEED))
    speed = max(0.5, min(speed, 8.0))

    route_path = os.path.join(SCRIPT_DIR, route_config)
    if not os.path.exists(route_path):
        print(f"route file not found: {route_path}", file=sys.stderr)
        sys.exit(1)

    print(f"loading route: {route_config}")
    route = load_route(route_path)
    print(f"raw points: {len(route)}")

    points = resample(route, speed, DT)
    total_m = sum(
        haversine(points[i][0], points[i][1], points[i + 1][0], points[i + 1][1])
        for i in range(len(points) - 1)
    )
    pace_min = int(1000 / speed // 60)
    pace_sec = int(1000 / speed % 60)
    print(f"speed: {speed:.1f} m/s, pace: {pace_min:02d}:{pace_sec:02d} /km")
    print(f"resampled: {len(points)} points, 1 pt/s, lap={total_m:.0f} m, {len(points)} s/lap")

    # Graceful shutdown
    running = True

    def on_signal(sig, frame):
        nonlocal running
        running = False

    signal.signal(signal.SIGTERM, on_signal)
    signal.signal(signal.SIGINT, on_signal)

    # Establish persistent connection
    dvt = create_dvt_service()
    print("location simulation ready")

    lap = 0
    i = 0
    started = time.time()

    try:
        while running:
            lat, lng = points[i]
            set_location(dvt, lat, lng)
            elapsed = int(time.time() - started)
            print(f"lap{lap} {i + 1}/{len(points)} ({lat:.6f},{lng:.6f}) elapsed={elapsed // 60:02d}:{elapsed % 60:02d}")
            i += 1
            if i >= len(points):
                lap += 1
                i = 0
            time.sleep(DT)
    except KeyboardInterrupt:
        pass
    except Exception as e:
        print(f"error: {e}", file=sys.stderr)
        sys.exit(1)
    finally:
        print("clearing simulated location...")
        clear_location(dvt)
        print("done.")


if __name__ == "__main__":
    main()
