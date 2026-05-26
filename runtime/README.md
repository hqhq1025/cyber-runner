# Cyber Runner Python Runtime

这个目录里的 Python 代码（main.py / init/ / run.py / driver/ / util/ /
config.* / route 数据）vendored 自
[BiancoChiu/iOSRealRun-cli-18](https://github.com/BiancoChiu/iOSRealRun-cli-18)
（再往前可追溯到 [iOSRealRun/iOSRealRun-cli-17](https://github.com/iOSRealRun/iOSRealRun-cli-17)），
**Mozilla Public License 2.0**。完整 license 在 `LICENSE-runtime`，原始
仓库 LICENSE 一字未改保留。

## 为什么要 vendor

Cyber Runner 直接 spawn 这里的 `main.py` 作为 GPS 模拟引擎。早先版本
靠"开发者本机另装一份 legacy run repo"来用，导致 sunshine（Windows 同学）
clone cyber-runner 时拿到一个 **runtime 缺失**的 repo，被迫自己装
pymobiledevice3，撞上 `>=5.0.4` 没锁上界 → 拿到 9.x → API breaking
change → 报 issue #1 说"无法实现实时 GPS 模拟"。

Vendor 进 repo 后：

- `git clone` 拿到完整可跑的代码
- `pyproject.toml` 严格 pin `pymobiledevice3==5.0.4`（已验证可用的版本）
- release bundle 时这个目录被打进 `.app` / `.msi`，首次启动 cp 到用户
  数据目录的 `runtime/`，然后跑 `uv sync` 起 `.venv`

## 修改约定

- **运行时逻辑的 bug 修复**（main.py / init/ / run.py 等）原则上**优先**
  往上游 [BiancoChiu/iOSRealRun-cli-18](https://github.com/BiancoChiu/iOSRealRun-cli-18)
  提 PR，再 sync 回这里；只有 Cyber Runner 私有改动（比如适配前端 IPC
  marker `已开始模拟跑步`）才直接改这里
- 改 `pyproject.toml` / `uv.lock` 后必须本地 `uv sync` 一遍，verify
  `from pymobiledevice3.services.dvt.instruments.location_simulation import LocationSimulation`
  仍 work（v5 → v9 之间 API 全变了，pin 是认真的）
- `__pycache__` / `.venv` / `.uv-cache` 已在外层 `.gitignore` 里

## 升级 pymobiledevice3 到 v9+

如果未来要跟住 upstream，需要同步改：

- `LocationSimulation.set()` 现在是 `async def` → `run.py` 里的调用全部
  改成 `await loc.set(...)`
- `DvtSecureSocketProxyService` 已被 `DvtProvider`
  (`pymobiledevice3.services.dvt.instruments.dvt_provider`) 替换 →
  `run.py` 改 `async with DvtProvider(rsd) as dvt, LocationSimulation(dvt) as loc:` 模式
- Windows 上要 admin + Microsoft Store 版 iTunes + wintun 驱动；老版本
  v5 的 tunnel 实现更宽松

详细 v5→v9 迁移路径参考 [issue #1](https://github.com/hqhq1025/cyber-runner/issues/1)
讨论区的调研笔记。
