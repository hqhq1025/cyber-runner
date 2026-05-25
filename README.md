# Cyber Runner

赛博校园跑 —— 你手机说在跑，你没动。

Cross-platform (macOS · Windows) iOS location simulator built with Tauri 2 +
Rust + Svelte 5. Reuses [`pymobiledevice3`](https://github.com/doronz88/pymobiledevice3)
to drive an iOS device's DVT `LocationSimulation` service along a configured
route.

## Status

Work in progress. Cross-platform compile verified on macOS + Windows (CI);
end-to-end (real iPhone GPS moves) verified on macOS. Windows real-device run
is the next milestone — see `GOAL.md` for the execution log.

## Layout

```
cyber-runner/
├── crates/
│   ├── cyber-runner-core/      # shared types, pure logic, no I/O
│   ├── cyber-runner-engine/    # process orchestration, log streaming,
│   │                           # BundledHelperEngine + LegacyHelperEngine
│   ├── cyber-runner-helper/    # standalone root binary listening on commands/
│   └── cyber-runner-platform/  # macOS / Windows privilege escalation
├── src-tauri/                  # Tauri host — productName "Cyber Runner",
│                               # bundle id dev.haoqing.cyber-runner
├── src/                        # Svelte 5 frontend
├── scripts/                    # drive_android.py + bootstrap helpers
└── docs/                       # research + plans
```

App data (logs, PID files, Python runtime venv) lives at:

- macOS:   `~/Library/Application Support/CyberRunner/`
- Windows: `%LOCALAPPDATA%\CyberRunner\` (= `~\AppData\Local\CyberRunner\`)
- Linux:   `~/.local/share/CyberRunner/`

## Toolchain

- Rust stable (≥ 1.79)
- Node 22+ (CI runs Node 22; older may work, untested)
- Python 3.11+ for the embedded engine at runtime (legacy run is tested on 3.13)

Platform-specific:

- **macOS** — Xcode Command Line Tools (for the Rust linker)
- **Windows** — [iTunes](https://www.apple.com/itunes/) installed (provides
  Apple Mobile Device Service, which `pymobiledevice3` talks to over usbmux)
- **All** — `uv` is recommended for managing the Python venv. The engine
  auto-installs deps on first run if `uv` is on `PATH`.

## Device setup (iPhone / iPad)

You'll do this once per device.

**Requirements**

- iPhone or iPad on iOS **17 or newer** (17 / 18 / 26 all confirmed)
- A real USB **data** cable (charge-only cables don't expose lockdownd)
- Only **one** iOS device connected at a time — extras will confuse usbmux

**Steps**

1. **Enable Developer Mode** on the device
   `Settings → Privacy & Security → Developer Mode → On`
   - This requires a reboot + your device passcode after the toggle
   - If the menu item is missing: plug the device in and run Cyber Runner
     once — `pymobiledevice3` auto-reveals the toggle, then retry
2. **Connect via USB**, unlock the device, and tap **Trust** when the
   "Trust This Computer?" prompt appears
3. (Windows only) make sure iTunes / Apple Mobile Device Service is running
   in the background — open iTunes once after install if it isn't

## Run (dev)

### macOS

```bash
npm install
npm run tauri dev
```

The first time you click **▶ 开始** the app shells out via `osascript … with
administrator privileges` to spawn the root helper — macOS will pop the
standard password prompt once per session.

### Windows

Open **PowerShell as Administrator**, then:

```powershell
npm install
npm run tauri dev
```

> Why admin? The Python engine needs to create a TUN device for the
> iOS 17+ DVT tunnel; that requires `Administrator`. In release builds
> the bundled UAC manifest handles this automatically, but `tauri dev`
> inherits whatever terminal it was launched from, so the terminal itself
> must be elevated.

### What you'll see

1. Click **▶ 开始** — the button changes to "准备中…" with a spinner
2. ~10–15 seconds later the iOS Developer Mode tunnel comes up, and the
   button changes to **✓ 已开始赛博跑步** (pulsing green). Your iPhone's
   `Maps.app` / target app should now show the simulated location moving
   along the configured route.
3. Click **■ 停止** to terminate the Python engine and restore real GPS.

## Build (release bundle)

```bash
npm run tauri build
```

Produces:

- **macOS** — `.app` + `.dmg` under `src-tauri/target/release/bundle/macos/`
  (unsigned; for personal use only — distributing to others requires Apple
  Developer ID)
- **Windows** — `.msi` + NSIS `.exe` under `src-tauri\target\release\bundle\`
  (unsigned; same caveat — Windows code-signing cert needed for SmartScreen)

## Known issues / 注意事项

- **Don't kill the window or unplug USB mid-run.** If the Python engine
  doesn't get to clean up, the device may stay stuck in simulated location.
  Recovery: reboot the iPhone.
- **Always click 停止 to exit.** Closing the window also triggers a clean
  shutdown via `Action::Shutdown` to the helper, but the explicit Stop is
  the safest path.
- **Tunnel startup takes 5–15 seconds.** The "准备中…" state covers this —
  it's not a hang. If it stays "准备中…" past 30s, check the log panel for
  `pymobiledevice3` errors (most often: Developer Mode off, or device
  locked).
- **Windows UAC manifest** is bundled for release `.msi` builds; `tauri dev`
  is not auto-elevated — must be launched from an Administrator terminal.
- **macOS:** if the password prompt is dismissed, the helper won't start —
  next click on ▶ 开始 will trigger another prompt.

## Architecture in one paragraph

The Tauri host (Rust) handles UI events and spawns
`cyber-runner-helper` — a small standalone root binary — via the
platform-specific elevation path (osascript on macOS, inherited-admin
`CreateProcess` on Windows). The helper polls `…/commands/request-*.json`,
dispatches each request to `RealEngine` (which spawns the Python
`main.py` subprocess), and writes `response-*.json` back. The helper holds
the only handle to the Python child, so **stop** = kill that child = real
GPS restored. See `docs/research/` for design notes.
