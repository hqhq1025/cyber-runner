# AGENTS.md

给 AI 助手的项目导览。架构与使用说明见 `README.md`。

## 结构速查

| 位置 | 内容 | 改动后跑 |
|---|---|---|
| `crates/cyber-runner-core` | 坐标系、路线解析、轨迹清理 (`track.rs`)、运动模型 (`motion.rs`) | `cargo test -p cyber-runner-core` |
| `crates/cyber-runner-engine` | `device.rs` 配对/预检，`ddi.rs` 镜像，`link.rs` 隧道，`runner.rs` 运行 actor | `cargo test -p cyber-runner-engine` |
| `crates/cyber-runner-cli` | 无界面调试工具 | `cargo run -p cyber-runner-cli -- devices` |
| `src-tauri/src` | `commands.rs` IPC，`routes.rs` 路线库，`settings.rs`，`lib.rs` 事件泵/菜单/崩溃恢复 | `cargo test -p cyber-runner-app` |
| `src/` | Svelte 5 + TS；`lib/api.ts` 是唯一的后端入口，`lib/mock.ts` 是浏览器模拟后端 | `npm run check` |

## 约定与陷阱

- `idevice` 精确 pin（`=0.1.68`），每个 0.1.x 都有破坏性变更；升级时同时检查
  `device.rs` / `link.rs` / `ddi.rs`。
- `idevice` 的 future 不是 `Send`（rustc 对 `Box<dyn ReadWrite>` 的推断限制），所以引擎在
  自己的线程 + `LocalSet` 里运行；不要把设备调用 `tokio::spawn` 到多线程 runtime。
- 位置必须是"距离"的函数，距离由真实 `dt` 积分。不要回到"每步 sleep 固定时间"的做法
  （旧版配速偏差 3–15% 的根因）。`measured_pace` 测试会拦。
- 路线在 `Track::new` 中去尖刺 + 平滑；UI 的预览和每圈长度都来自同一份 `Track`，
  保持一致。
- `Snapshot.simulating` 表示"手机可能还在模拟定位"，只有成功清除才会置 false；
  `src-tauri` 用它维护崩溃恢复标记 `simulating.json`。
- macOS 上系统默认的 Quit 菜单项会绕过 Tauri 的退出事件，所以菜单是自定义的；
  别换回 `PredefinedMenuItem::quit`。
- Webview 目标：macOS WKWebView（旧系统 ≈ Safari 16）+ Windows WebView2。新 CSS/HTML
  特性要有回退（例如用 `prefers-color-scheme` 而不是只用 `light-dark()`）。
- 只做桌面横屏。

## 不要乱碰

- `GOAL.md`、`docs/research/`：owner 的历史记录与调研。
- `scripts/drive_android.py`：已关闭的 Android 实验线。

## 协作

Owner：hqhq1025。走分支 + PR，commit 用 conventional commits（`fix(engine): …`）。
