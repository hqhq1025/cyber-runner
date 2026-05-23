# Cyber Runner

赛博校园跑 —— 你手机说在跑，你没动。

Cross-platform (macOS · Windows) iOS location simulator built with Tauri 2 + Rust + Svelte 5.

Reuses the open-source [`pymobiledevice3`](https://github.com/doronz88/pymobiledevice3)
to drive an iOS device's DVT LocationSimulation service along a configured
route.

## Status

Work in progress. See `docs/plans/2026-05-22-fake-run-cross-platform.md` in
the sibling `run/` repository for the original plan (project was codenamed
"fake-run" pre-rebrand), and `GOAL.md` here for the execution log.

## Layout

```
cyber-runner/
├── crates/
│   ├── cyber-runner-core/      # shared types, pure logic, no I/O
│   ├── cyber-runner-engine/    # process orchestration, log streaming,
│   │                           # LegacyHelperEngine bridge
│   └── cyber-runner-platform/  # macOS / Windows privilege escalation
├── src-tauri/                  # Tauri host — productName "Cyber Runner",
│                               # bundle id dev.haoqing.cyber-runner
├── src/                        # Svelte 5 frontend
├── scripts/                    # drive_android.py + bootstrap helpers
└── docs/                       # research + plans
```

App data lives at:
- macOS: `~/Library/Application Support/CyberRunner/`
- Windows: `%LOCALAPPDATA%\cyber-runner\`

## Toolchain

- Rust stable (≥ 1.79)
- Node 20+
- Python 3.11+ (for the embedded engine at runtime)

## Build

```bash
npm install
npm run tauri dev       # dev window
npm run tauri build     # release .app / .msi
```
