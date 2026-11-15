# Cyber Runner

赛博校园跑 —— 让 iPhone 沿着路线"跑起来"。macOS · Windows 桌面 app。

**v0.2 是完全重写**：纯 Rust 直接和手机通信，不再需要 Python、不再需要管理员 / root
密码、不再有后台 helper 进程。Windows 和 macOS 走同一套代码。

## 给使用者

### 需要准备

| | macOS | Windows |
|---|---|---|
| 系统 | macOS 11+ | Windows 10 / 11 |
| 苹果驱动 | 系统自带 | 微软商店安装 **Apple Devices**（或 iTunes），装好打开一次 |
| 管理员权限 | 不需要 | 不需要 |
| 手机 | iPhone / iPad，**iOS 17 及以上** | 同左 |
| 数据线 | 能传数据的线（仅充电线不行） | 同左 |

### 三步开始

1. 用数据线连上手机并解锁，打开 Cyber Runner，右上角显示手机名字即连接成功。
2. 像用跑步机一样：选路线（内置 北邮 · 西土城 / 沙河，或导入自己的），用 − / + 调配速
   （5 / 4 / 3 分配快捷档）和跑多远（默认 5 km 档，实跑 6 km，多 1 km 防 App 少记）。
3. 点 **开始跑步**。第一次会依次：
   - 手机弹「要信任此电脑吗？」→ 点信任、输密码；
   - 若提示开发者模式：设置 → 隐私与安全性 → 开发者模式 → 打开并重启；
   - 自动下载并挂载开发者磁盘镜像（约 17 MB，只需一次，手机重启后自动重新挂载）。

跑完点 **结束**，手机立即恢复真实定位。设置了目标时，到达后会弹系统通知、响提示音，默认
**停在终点**，方便你先在校园跑 App 里点结束，再回来恢复定位。

配速永远不会快过 3'05"/km（校园跑 App 通常判快于 3'00" 无效）；每次跑的轨迹、起点、每圈
快慢都随机，不会两次一样，但距离按实际发出的轨迹精确计算。

### 导入路线

「导入路线」或直接把文件拖进窗口。支持 GPX、KML、GeoJSON、旧版 txt（`{"lng":..,"lat":..}`）
和每行一个「经度,纬度」的文本。用百度地图坐标拾取器取的点选「百度」坐标系，高德/腾讯选
「高德/腾讯」，手表导出的 GPX 自动识别为 GPS 坐标。

路线会被自动清理：去掉重复点、去掉取点失误造成的"折返尖刺"、平滑成真人会跑的线。
导入预览里显示的就是实际会跑的线，每圈长度也按它计算。

### 常见问题

- **定位卡在模拟位置**：设置 → 恢复真实定位；或重启手机。异常退出后再次打开 app 时，
  只要手机连着，会自动恢复。
- **Windows 显示"缺少苹果设备服务"**：安装 Apple Devices / iTunes 并打开一次。
- **一直"建立定位通道"**：拔插数据线、保持手机解锁，仍不行重启手机。
- 遇到问题：底部「日志」→「复制全部」发给开发者。

## 给开发者

```
crates/
  cyber-runner-core/    纯逻辑：坐标系转换、路线解析、轨迹清理/平滑、按时间积分的运动模型（全部单测）
  cyber-runner-engine/  设备层：usbmuxd → 配对 → 开发者模式 → DDI → CoreDeviceProxy 软件隧道 → DVT 定位
  cyber-runner-cli/     命令行：cyber-runner devices | run | restore
src-tauri/              桌面宿主：路线库、设置持久化、事件推送、退出时恢复定位
src/                    Svelte 5 + TypeScript 界面
```

关键设计：

- **一个进程，无特权**。设备 I/O 跑在引擎自己的线程（单线程 tokio runtime）里，UI 通过
  命令 / 状态快照和它通信。`idevice` 的用户态 TCP 栈替代了 pymobiledevice3 的 TUN 隧道，
  所以不需要 root / 管理员。
- **配速精确**。位置是"已跑距离"的函数，距离由真实流逝时间积分得到，与发送频率、USB
  延迟、系统计时精度无关。`cargo test -p cyber-runner-core measured_pace` 按"校园跑 App 以
  1 Hz 采样 GPS 求和"的方式验证，误差 < 0.5%。
- **恢复定位有三层保障**：结束按钮 / 退出菜单（macOS 自定义了 Cmd+Q，系统默认的会跳过清理）
  → 当前连接失败时重新连接再清 → 仍失败则留下标记，下次启动检测到手机自动恢复。

### 开发

```bash
npm install
npm run tauri dev          # 桌面 app
npm run dev                # 仅界面，浏览器里用模拟数据（?autostart、?nodevice、?fail）
cargo run -p cyber-runner-cli -- devices
```

提交前：

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
npm run check && npm run build
```

### 打包

在要用的那台电脑上本地运行 `npm run tauri build`（Mac 上出 .app/.dmg，Windows 上出 .msi/.exe）。安装包未签名：
macOS 首次打开需右键 → 打开；Windows SmartScreen 选「仍要运行」。

## 致谢

设备协议基于 [idevice](https://github.com/jkcoxson/idevice)。开发者磁盘镜像来自
[doronz88/DeveloperDiskImage](https://github.com/doronz88/DeveloperDiskImage)。内置路线
数据来自 [BiancoChiu/iOSRealRun-cli-18](https://github.com/BiancoChiu/iOSRealRun-cli-18)。
