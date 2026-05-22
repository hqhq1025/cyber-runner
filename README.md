# fake run

Cross-platform (macOS + Windows) iOS location simulator built with Tauri + Rust.

Reuses the existing Python engine (`runtime/`) that talks to the device via
[`pymobiledevice3`](https://github.com/doronz88/pymobiledevice3).

## Status

Work in progress. See `docs/plans/2026-05-22-fake-run-cross-platform.md` in
the sibling `run/` repository for the implementation plan.

## Layout

```
fake-run/
├── crates/
│   ├── fake-run-core/      # shared types, pure logic, no I/O
│   ├── fake-run-engine/    # process orchestration, log streaming
│   └── fake-run-platform/  # macOS / Windows privilege escalation
├── src-tauri/              # Tauri host (Rust)
├── src/                    # Svelte frontend
├── runtime/                # Python engine (vendored from sibling repo)
└── tests/                  # cross-cutting integration tests
```

## Toolchain

- Rust stable (≥ 1.79)
- Node 20+
- Python 3.11+
