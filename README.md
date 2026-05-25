# Cyber Runner

赛博校园跑 —— 你手机说在跑，你没动。

跨平台（macOS · Windows）的 iOS 定位模拟器，用 Tauri 2 + Rust + Svelte 5
写的桌面 app。底层复用开源的
[`pymobiledevice3`](https://github.com/doronz88/pymobiledevice3)
驱动 iOS 设备的 DVT `LocationSimulation` 服务，按预设路线推 GPS 坐标。

## 当前状态

开发中。跨平台编译在 macOS + Windows 都过 CI；端到端真机（iPhone GPS
真的动）在 macOS 上验证 OK；Windows 真机跑通是下一个里程碑。执行进度看
`GOAL.md`。

## 目录结构

```
cyber-runner/
├── crates/
│   ├── cyber-runner-core/      # 纯逻辑，types / 坐标转换 / 路径平滑，无 I/O
│   ├── cyber-runner-engine/    # Python 子进程编排、日志流、
│   │                           # BundledHelperEngine + LegacyHelperEngine
│   ├── cyber-runner-helper/    # 独立 root binary，监听 commands/ 文件 IPC
│   └── cyber-runner-platform/  # macOS / Windows 提权抽象
├── src-tauri/                  # Tauri host（productName "Cyber Runner",
│                               # bundle id dev.haoqing.cyber-runner）
├── src/                        # Svelte 5 前端
├── scripts/                    # drive_android.py 等 bootstrap 脚本
└── docs/                       # 调研笔记 + plan
```

App 运行时数据（日志、PID、Python venv）落在：

- **macOS**：`~/Library/Application Support/CyberRunner/`
- **Windows**：`%LOCALAPPDATA%\CyberRunner\`（即 `~\AppData\Local\CyberRunner\`）
- **Linux**：`~/.local/share/CyberRunner/`

## 开发依赖

- **Rust** stable（≥ 1.79）
- **Node** 22+（CI 用 22，更老版本可能能跑但没测）
- **Python** 3.11+（runtime 里嵌入的引擎需要；legacy run 实测到 3.13）

分平台还要：

- **macOS** — Xcode Command Line Tools（Rust 用的 linker）
- **Windows** — 装 [iTunes](https://www.apple.com/itunes/)（提供 Apple Mobile
  Device Service，`pymobiledevice3` 通过 usbmux 跟它对话）
- **通用** — 推荐装 `uv` 管 Python venv。第一次启动时如果 `PATH` 里有 `uv`，
  引擎会自动装好依赖。

## 手机端配置（iPhone / iPad）

**每台设备做一次就够。**

**硬件 / 系统要求**

- iPhone 或 iPad，**iOS 17 及以上**（17 / 18 / 26 均实测可用）
- **数据线**（不是仅充电线，否则手机的 lockdownd 服务连不上来）
- 同一时间**只能接一台** iOS 设备 —— 多台会让 usbmux 蒙圈

**步骤**

1. **开启开发者模式**
   `设置 → 隐私与安全性 → 开发者模式 → 打开`
   - 打开后需要**重启 + 输入密码**才生效
   - 找不到这个菜单？把设备 USB 接上，先跑一次 Cyber Runner，
     `pymobiledevice3` 会自动唤出这个开关，然后再重试
2. **USB 连上电脑 → 解锁手机 → 弹"是否信任此电脑"时点信任**
3. **（仅 Windows）** 确保 iTunes / Apple Mobile Device Service 在后台
   运行 —— 装完 iTunes 后至少打开过一次就行

## 启动（开发模式）

### macOS

```bash
npm install
npm run tauri dev
```

第一次点 **▶ 开始** 会触发 `osascript … with administrator privileges`
拉起 root helper —— 会弹一次系统密码框，输入开机密码即可。每个 session
弹一次。

### Windows

**用管理员身份打开 PowerShell**，然后：

```powershell
npm install
npm run tauri dev
```

> **为什么需要管理员？** Python 引擎要给 iOS 17+ DVT tunnel 建一个 TUN
> 虚拟网卡，这一步需要 `Administrator` 权限。Release 打包出来的 `.msi`
> 装的 app 有 UAC manifest 会自动提权；但 `tauri dev` 是继承你启动它的
> 终端的权限，所以**终端本身**必须是管理员。

### 你应该看到什么

1. 点 **▶ 开始** → 按钮变成"准备中…" + 转圈
2. 大约 **10–15 秒** 后 iOS Developer Mode tunnel 建立成功，按钮变成
   **✓ 已开始赛博跑步**（绿色脉冲动画）。打开 iPhone 的"地图"app
   或者校园跑那个 app，能看到位置沿预设路线在动了
3. 点 **■ 停止** → Python 引擎被干净 kill，真实 GPS 恢复

## 打包发布版

```bash
npm run tauri build
```

产出：

- **macOS** — `.app` + `.dmg` 在 `src-tauri/target/release/bundle/macos/`
  （未签名，仅自用；要发给别人要 Apple Developer ID）
- **Windows** — `.msi` + NSIS `.exe` 在 `src-tauri\target\release\bundle\`
  （未签名；Windows SmartScreen 会拦，要 Windows 代码签名证书才能消除）

## 常见问题 / 注意事项

- **不要中途关窗口 / 拔 USB 线。** Python 引擎来不及清理的话，手机会卡在
  模拟位置状态。**恢复方法：重启 iPhone。**
- **退出请用 ■ 停止按钮。** 关窗口也会触发 `Action::Shutdown` 让 helper
  干净退出，但显式点停止最稳。
- **Tunnel 启动 5–15 秒属正常。** "准备中…" 这段时间不是卡住。如果超过
  30 秒还在准备中，看下日志面板有没有 `pymobiledevice3` 报错 —— 最常见
  是开发者模式没开、或者手机锁屏了。
- **macOS：** 密码框点取消 helper 起不来 —— 再点 ▶ 开始会重新弹一次。
- **Windows：** UAC manifest 只对 release `.msi` 装的 app 生效；
  `tauri dev` 必须在管理员终端里跑。

## 一段话讲架构

Tauri host（Rust）负责 UI 事件和 spawn `cyber-runner-helper` —— 一个
独立的 root binary —— 通过平台提权（macOS 用 osascript，Windows 通过
继承管理员权限的 `CreateProcess` + detach）拉起。Helper 轮询
`…/commands/request-*.json`，把每个请求 dispatch 给 `RealEngine`（在
helper 进程内 spawn `main.py` Python 子进程），结果写回
`response-*.json`。**Helper 是 Python 子进程的唯一持有者**，所以点
**停止** = 干掉那个 Python 子进程 = 真实 GPS 恢复。详细设计看
`docs/research/`。

## 给 AI 助手用

如果你用 Claude Code / Cursor / Codex 等 AI agent 帮忙配置或调试，根目录
有 `AGENTS.md`，里面写了 agent 该问用户什么、查什么、不该乱碰什么。
