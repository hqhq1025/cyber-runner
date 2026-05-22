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

### 已完成

- `c30b450` Phase 0 · 仓库 + Rust workspace + 3 个 placeholder crate
- `1ce9c05` Phase 0 · Tauri 2 host + Svelte 5 前端 + 图标 + CI yml
- `5379f4a` Phase 1 · Swift `RunControlCore` + Python 纯函数全部移植，60 tests pass（6 个城市的 BD-09→WGS-84 golden value 对齐到 1e-9）
- `ef8c12f` Phase 1 · 修 clippy excessive_precision
- `a62c364` Phase 2 scaffold · Engine trait + python_runner，6 tests
- `69c4dc4` Phase 2 · env_setup + cleanup + RealEngine，14 tests
- `5008315` Phase 3 · macOS osascript + Windows UAC manifest + embed-manifest，15 tests（subagent）
- `3195af7` Phase 4 · Svelte 5 UI 1:1 移植 + 4 个 mock Tauri commands，8 tests + 190 files type-check 0 errors（subagent）
- `935be03` Android 调研 + emulator 架构 B 实测通过

### Android 线收尾（2026-05-22）—— **CLOSED**

**结论**：对**用户具体场景（北邮校园跑微信小程序）无可行的非 root 方案**。

**实测过程**：
1. ✅ emulator 上 adb 注入 GPS 完美工作（架构 B 验证）
2. ✅ 真机 OPPO PJW110 Android 16 ColorOS 16，自加 manifest receiver 重 build 后 shell broadcast 也工作
3. ✅ 系统 GPS provider 被改后**百度地图正确显示假位置**
4. ❌ **微信小程序仍显示真实位置**——根因：`wx.getLocation()` → `TencentLocationManager`，腾讯 SDK 优先用 WiFi MAC + 基站 ID 联网反查（`apis.map.qq.com`），不读 Android 系统 GPS provider
5. 研究 `docs/research/wechat-location-bypass-2026-05-22.md` 确认：绕开腾讯 SDK 唯一可行路径是 **root + Magisk + LSPosed + Shadow 模块**
6. OPPO PJW110 是 Find X8 系列，2023+ 机型 OPPO 已停发解锁码，社区方案门槛极高；且 root 后银行/支付宝/微信支付会拒登；user 日用主力机，**性价比不值得**

**保留的产物**（不删，有价值）：
- `scripts/drive_android.py`：北邮西土城路线 BD-09→WGS-84 + adb broadcast，对**非微信定位 APP** 仍直接可用
- `docs/research/android-fake-run-2026-05-22.md`：整体调研
- `docs/research/android-arch-b-smoke-test-2026-05-22.md`：emulator 复现脚本
- `docs/research/wechat-location-bypass-2026-05-22.md`：dead-end 分析 + 5 方案对比

**删的**：`external/dlut-FakeRun/` 整目录（已 gitignored，本地删除省盘）

### 当前重新聚焦：iOS 主线 Phase 5

集成真 RealEngine 到 Tauri commands，替换 Phase 4 留下的 mock，端到端串通"GUI 点开始 → spawn python main.py → 日志流到 UI"。


