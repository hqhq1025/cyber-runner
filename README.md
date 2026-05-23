# Cyber Runner

Cross-platform (macOS + Windows) iOS location simulator built with Tauri + Rust.

Codename **赛博校园跑** — your phone says you're running, you're not.

Reuses the existing Python engine that talks to the device via
[`pymobiledevice3`](https://github.com/doronz88/pymobiledevice3).

## Status

Work in progress. See `docs/plans/2026-05-22-fake-run-cross-platform.md` in
the sibling `run/` repository for the implementation plan and `GOAL.md`
in this repo for the execution log.

## Layout

```
fake-run/                       (repo dir — kept for git history)
├── crates/
│   ├── fake-run-core/          # shared types, pure logic, no I/O
│   ├── fake-run-engine/        # process orchestration, log streaming
│   └── fake-run-platform/      # macOS / Windows privilege escalation
├── src-tauri/                  # Tauri host (Rust) — productName "Cyber Runner"
├── src/                        # Svelte 5 frontend
└── docs/                       # research + plans
```

> Internal Rust crate names still use the `fake-run-` prefix because
> renaming them would churn every import. They're invisible to end users.

## Toolchain

- Rust stable (≥ 1.79)
- Node 20+
- Python 3.11+
