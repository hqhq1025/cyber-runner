# AGENTS.md

给 AI 助手（Claude Code / Cursor / Codex / 其它）用的项目导览。新人用 AI
agent 帮自己配置 Cyber Runner 时，agent 先读这个文件。

---

## 这是什么项目

`Cyber Runner` 是个跨平台（macOS · Windows）桌面 app，让 iPhone / iPad
按预设路线伪造 GPS（DVT `LocationSimulation`）。

技术栈：Rust workspace（4 crates）+ Tauri 2 host + Svelte 5 前端 +
嵌入式 Python 引擎（复用 `pymobiledevice3`）。架构概览看 `README.md`
末尾"一段话讲架构"。详细执行日志看 `GOAL.md`。

**当前进度**（agent 想验证就 `git log --oneline -10`）：
- macOS 端到端（真 iPhone GPS 真的动）已通
- Windows 跨平台编译 CI 已绿（`cargo check x86_64-pc-windows-msvc`）
- Windows 真机跑通是下一步

---

## 帮新人首次配置时，先问这 5 件事

如果用户说"帮我把这个跑起来"，先问：

1. **你的 OS 是？**（macOS / Windows / Linux）—— 三平台启动姿势不同
2. **要用真机吗？** 如果是，你的设备 iOS 版本是？（必须 ≥ 17）
3. **已装 Node 22 / Rust stable / Python 3.11+ 了吗？** 缺哪个就先补
4. **（Windows only）装 iTunes 了吗？** `pymobiledevice3` 通过 Apple
   Mobile Device Service 跟手机说话，没 iTunes 没这服务
5. **你的设备开发者模式开了吗？** 路径：`设置 → 隐私与安全性 → 开发者模式`

不要直接 `npm install && npm run tauri dev` 一把梭 —— 上面任何一项缺
都会让用户在一个 confusing 的地方卡住。

### Happy path（5 项都齐了之后）

```bash
git clone git@github.com:hqhq1025/cyber-runner.git
cd cyber-runner
npm install              # 装前端 + tauri-cli
npm run tauri dev        # mac 直接跑；Windows 从管理员 PowerShell 跑
```

第一次点 ▶ 开始：
- macOS 弹密码框 → 输开机密码 → helper 起来
- 12s 左右按钮变 "✓ 已开始赛博跑步"
- iPhone 上看地图 / 校园跑 app 位置在动

---

## 常见 fail 信号 → root cause

| 用户报告 | 大概率原因 | 验证 / 处理 |
|---|---|---|
| 点 ▶ 后窗口一直"准备中..."超 30s | 手机开发者模式没开 / 手机锁屏 / USB 是仅充电线 | 查日志面板 `pymobiledevice3` 报错 |
| Windows 上 `npm run tauri dev` 起来 app 但点 ▶ 立刻报权限错 | PowerShell 不是管理员身份起的 | 关掉，用"以管理员身份运行" PowerShell 重起 |
| Windows 报 `usbmuxd` / lockdownd 连不上 | 没装 iTunes / Apple Mobile Device Service 没在跑 | 装 iTunes，至少开过一次 |
| macOS 点停止后手机定位还在动 | 之前留下了 zombie root helper（旧 tauri dev 没清干净） | `ps aux \| grep cyber-runner-helper`，`sudo kill` 掉所有；再起 |
| `npm run check` 在本地过、CI 挂在 svelte-check | 通常是 `$derived` 引用了文件后面才定义的变量（TDZ）—— Vite dev 不 type-check 所以本地 tauri dev 没炸 | 把声明提前到 use 之前 |
| 编译 Windows target 在 mac 上失败 | mingw / MSVC SDK 在 mac 上没法装；只能 `cargo check`（不 link） | CI 在 windows-2022 runner 上跑真 build |

---

## Codebase orientation

```
crates/
  cyber-runner-core/        ← 纯逻辑（types、BD-09→WGS-84、路径平滑）。改这里要跑 cargo test -p cyber-runner-core
  cyber-runner-engine/
    real.rs                 ← 在 helper 进程内 spawn Python main.py
    bundled_helper.rs       ← Tauri host 端的 file-IPC 客户端 + spawn_helper_platform（macOS/Windows/Linux cfg 分支）
    python_runner.rs        ← venv 路径 / process_alive / kill
    env_setup.rs            ← uv sync / 系统 Python 探测
  cyber-runner-helper/
    main.rs                 ← root daemon：单实例 guard、atomic request claim、SIGTERM/ctrl_c 处理
  cyber-runner-platform/    ← osascript / UAC 提权抽象

src-tauri/src/
  lib.rs                    ← Tauri setup，default_base_dir 用 dirs::data_local_dir
  commands.rs               ← Tauri commands（前端调用入口）
  state.rs                  ← AppState + 日志 ringbuffer

src/                        ← Svelte 5（runes 语法，注意 $state / $derived TDZ）
```

**改某 crate 前先 `cargo test -p <name>` 跑一遍**，知道修改前的 baseline。

---

## Dev workflow

每次本地改完，PR / push 前自己跑一遍 CI 等价命令：

```bash
# 1) Rust 三件套（CI 在 mac + win 都跑）
cargo fmt --all -- --check
cargo clippy -p cyber-runner-core -p cyber-runner-engine -p cyber-runner-platform --all-targets -- -D warnings
cargo test -p cyber-runner-core -p cyber-runner-engine -p cyber-runner-platform --no-fail-fast

# 2) 前端（CI 在 mac + win 都跑）
npm run check     # svelte-check，会抓 TDZ / 类型错误，比 Vite dev 严格
npm run build

# 3) Tauri host 编译
cargo check -p cyber-runner-app

# 4) Windows 跨平台编译验证（只 check，不 link）
cargo check -p cyber-runner-helper -p cyber-runner-engine --target x86_64-pc-windows-msvc
```

写新 platform-specific 代码记得：
- `#[cfg(target_os = "macos")]` / `#[cfg(target_os = "windows")]` /
  `#[cfg(not(any(...)))]` Linux fallback 三路都覆盖
- 引入 Windows API（`std::os::windows::*`）的 `use` 也要 cfg gate，
  否则 mac 上 unused import warning
- `tokio::process::Command` 有 inherent `creation_flags()`，**不需要**
  import `std::os::windows::process::CommandExt`

---

## CI

`.github/workflows/ci.yml` 在 macOS-14 + Windows-2022 上跑：

- Rust：fmt check / clippy `-D warnings` / cargo test
- Frontend + Tauri：npm ci / svelte-check / npm build / cargo check (tauri host)

push 到 main 或开 PR 都触发。fail 立刻看 `gh run view <id> --log-failed`。

---

## 不要乱碰

| 文件 / 目录 | 为什么不动 |
|---|---|
| `GOAL.md` | 项目 owner 的执行日志，每完成一个 phase 才追加 commit hash。Agent 不要重写，可以读。 |
| `docs/research/` | 调研档案，包括 Android 线 dead-end 分析。读可以，改要 owner 同意。 |
| `scripts/drive_android.py` | Android 实验产物（线已关闭，见 `docs/research/wechat-location-bypass-2026-05-22.md`）。保留有 reference value，但不要动 / 不要试图复活 Android 线。 |
| `~/Library/Application Support/CyberRunner/runtime/` (mac) / `%LOCALAPPDATA%\CyberRunner\runtime\` (win) | Python venv 落地处。**不在 git 里**。第一次跑 app 自动创建。如果坏了，删整个 `runtime/` 让 app 重建。 |
| `~/Library/Application Support/CyberRunner/helper.pid` | root-owned，可能是 stale。改 / 删它前先 `ps aux \| grep cyber-runner-helper` 确认是不是真死了再动。 |
| Legacy `~/Documents/Github/run/` repo | 是 Cyber Runner 的 macOS-only 前身（Swift + Python）。Python `init/` `route.py` `run.py` 是 Cyber Runner 直接复用的代码源 —— 读可以参考，但**所有改动放在 cyber-runner repo 里**，不要往 legacy run 提交。 |

---

## 已知架构陷阱（前人踩过）

1. **Multi-helper zombie**：osascript 拉起的 root helper 不是 tauri-app 的
   子进程，tauri dev kill 时 helper 不会一起死。下次 tauri 起来又拉一个新
   helper，两个都监听 `commands/`，同一 request 被处理 2 次，spawn 2 个
   Python，停止只能 kill 1 个 → 定位停不下来。
   **现在的修法**：helper 启动时单实例 guard + handle_request 用
   `fs::rename` 原子认领 request 文件。
   **遗产**：如果你看到 stale `helper.pid` 指向已死的 root PID，正常 —— 新
   helper 会无视并覆写。

2. **Svelte 5 runes TDZ**：`$derived` 是 const 绑定，必须先声明后使用。
   Vite dev 不做 type check，所以 `tauri dev` 跑得起来 ≠ 代码对。push
   前用 `npm run check` 复现 CI 的 svelte-check。

3. **Windows TUN device 权限**：`pymobiledevice3 lockdown start-tunnel`
   要创虚拟网卡，必须 `Administrator`。Release 用 UAC manifest 兜底；
   dev 模式只能从管理员 PowerShell 起 `tauri dev`。

4. **不要用 osascript 起 helper 时加 nohup**：osascript admin 给的不是真
   TTY，nohup 在 `tcgetattr` 失败立刻退，helper 起不来。当前用
   `( cmd </dev/null >>log 2>&1 & )` 模式（subshell + 重定向 stdin/out）
   绕开。详细注释看 `bundled_helper.rs::spawn_helper_platform` macOS 分支。

---

## 跟项目 owner 协作时

Owner 是 [@hqhq1025](https://github.com/hqhq1025)。沟通约定：

- 大改架构 / 引入新依赖 / 改 CI matrix 前先开 issue 或 PR draft 问一下
- bug fix / 小重构 / 文档改善可以直接 PR
- 不要往 `main` 直接 push（除非 owner 同意）—— 走 PR + CI 绿再合
- Commit message 用 conventional commits + scope，例如：
  - `fix(helper): single-instance guard`
  - `feat(ui): preparing→running state`
  - `docs: expand README device setup`
