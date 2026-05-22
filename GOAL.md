# fake-run · Goal-Mode Plan

**Started:** 2026-05-22

## Goal

把 `~/Documents/Github/fake-run/` 推到一个 **"在 mac 上 `cargo tauri dev` 启动后，点'开始'能 spawn Python 引擎并把日志流到 UI 上"** 的端到端 MVP，外加全套单元测试。Windows 这一轮只保证编译过 + UAC manifest 就绪，真机验证留到下一轮。

## Out of scope (this run)

- Phase 5：把 python-build-standalone 嵌进发行包（影响 ~100 MB 下载，等用户确认许可证/分发策略）
- Phase 6：DMG/MSI 签名（需要 Apple Developer ID + Windows 代码签名证书，花钱+人工）
- Phase 4 UI 大改：先按现 Swift 布局 1:1 移植，留扁平的"保留现有布局只换皮"方向（用户先前已默许）
- 推到 GitHub（用户明确说不推）

## Decisions I'll make alone

- **macOS 特权**：先用 `osascript ... administrator privileges`（每次 start 弹一次密码框），等 UX 不满意再补 `SMAppService.daemon`。
- **Windows 特权**：UAC manifest 整 app 以 Administrator 启动。
- **随机数对齐策略**：纯确定性函数（坐标转换、固定步长插值）跟 Python 走 golden value；随机扰动只测属性（点数守恒、偏移上限），不强求与 Python 数值一致。
- **错误处理**：核心库走 `thiserror` 枚举；Tauri command 层用 `Result<T, String>` 把错误转字符串给前端（Tauri 2 默认）。
- **日志格式**：保留 Swift 现有的 `===== YYYY-MM-DD HH:MM:SS start =====` 分隔符，方便复用既有 PID 探测 / 时间推断逻辑。

## Stop conditions (will pause and ask)

- 真的需要花钱（证书、付费 API）
- 需要部署到外部系统（GH 推送、远程服务器）
- 写完发现整体架构不对、要大改方向
- 卡住超过 30 分钟修不动

## Execution log

每完成一个 sub-task commit 一次，commit hash 记在这里。
