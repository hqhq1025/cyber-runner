// Pre-build cleanup: remove runtime/{.venv,__pycache__,.uv-cache} so
// `npm run tauri build` doesn't snapshot a developer's local venv into
// the bundled .app / .msi. If we don't do this:
//   - .app balloons by hundreds of MB (full site-packages)
//   - reinstall scripts (`rm -rf "/Applications/Cyber Runner.app"`) hit
//     symlinks / nested dirs / weird perms in venv internals and fail
//     mid-flight, leaving a broken half-removed app whose executable is
//     missing — macOS then refuses to launch it as "damaged / incomplete"
//   - users get a working venv in user data dir anyway via
//     ensure_runtime_files + ensure_deps
//
// Cross-platform: pure node, no shell. Tied into `npm run build` so it
// runs both via `npm run tauri build` and via direct CI `npm run build`.
import { rmSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = join(dirname(fileURLToPath(import.meta.url)), "..");
const runtime = join(repoRoot, "runtime");

const toClean = [".venv", "__pycache__", ".uv-cache"];
for (const sub of toClean) {
  rmSync(join(runtime, sub), { recursive: true, force: true });
}
console.log(`cleaned runtime/{${toClean.join(",")}}`);
