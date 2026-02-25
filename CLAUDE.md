# MyWallpaper Desktop (Tauri v2)

Animated wallpaper application — window injected behind desktop icons on Windows.
Frontend loaded remotely from `dev.mywallpaper.online` (no local build).

## Commands

```bash
npm install
npm run tauri:dev          # Dev mode (remote frontend)
npm run tauri:build        # Local release build
npm run tauri:build:debug  # Debug build with devtools
```

## Releasing

**Releases are fully automated via GitHub Actions. NEVER bump versions manually.**

```bash
# Dev release (fast build, devtools enabled, pre-release)
gh workflow run "Desktop Release" --field bump=patch --field mode=dev

# Production release (optimized, LTO, stripped)
gh workflow run "Desktop Release" --field bump=patch --field mode=prod
```

The CI automatically:
1. Bumps version in `tauri.conf.json`, `Cargo.toml`, `package.json`
2. Commits `release: desktop vX.Y.Z` and tags `vX.Y.Z` (or `vX.Y.Z-dev`)
3. Builds Windows in parallel
4. Signs updater artifacts with minisign
5. Generates `latest.json` updater manifest
6. Publishes GitHub release

**bump options**: `patch` (1.0.X+1), `minor` (1.X+1.0), `major` (X+1.0.0)

## Architecture

```
src-tauri/src/
├── main.rs            # Entry point (windows_subsystem)
├── lib.rs             # App init, plugins, window setup, invoke_handler
├── commands.rs        # Tauri IPC command wrappers
├── commands_core.rs   # Platform-independent business logic + types
├── system_monitor.rs  # System data collection (CPU, memory, battery, disk, network)
├── tray.rs            # System tray (quit only)
└── window_layer.rs    # Desktop injection + mouse engine + visibility watchdog
```

### Window Layer System (`window_layer.rs`)

The core of the app. Three subsystems:

1. **WorkerW Injection** — Detects OS architecture (Win11 24H2+ vs Legacy), injects WebView as child of WorkerW/Progman with correct Z-order
2. **Mouse Hook** — Low-level `WH_MOUSE_LL` hook with MSAA-based icon detection (`ROLE_SYSTEM_LISTITEM = 34`). State machine: IDLE/NATIVE/WEB. Forwards web clicks to `Chrome_RenderWidgetHostHWND`
3. **Visibility Watchdog** — Polls foreground window every 2s, emits `wallpaper-visibility` event when fullscreen app covers wallpaper (multi-monitor aware)

### System Monitor (`system_monitor.rs`)

Exposes system data to widgets via Tauri IPC:

- **Categories**: `cpu`, `memory`, `battery`, `disk`, `network`
- **One-shot**: `get_system_data(categories)` returns filtered `SystemData`
- **Real-time**: Background thread polls every 3s, emits `system-data-update` event
- **Permission-gated**: Frontend filters data per widget based on manifest capabilities

### Tauri Commands (IPC)

| Command | Description |
|---|---|
| `get_system_info` | OS, arch, app/Tauri version |
| `get_system_data` | CPU, memory, battery, disk, network (filtered by categories) |
| `subscribe_system_data` | Update monitor poll categories for real-time updates |
| `check_for_updates` | Check GitHub releases (supports custom endpoint for pre-release) |
| `download_and_install_update` | Download + install with progress events |
| `restart_app` | Restart to apply update |
| `open_oauth_in_browser` | Open OAuth URL in default browser |
| `reload_window` | Emit reload event to frontend |
| `set_desktop_icons_visible` | Show/hide native desktop icons (Windows: ShowWindow) |

### Safety

- `restore_desktop_icons()` runs on both `ExitRequested` and tray quit — icons always restored
- `ICONS_RESTORED` atomic flag prevents double-restore

### Auto-Updater

- Endpoint: `https://github.com/MyWallpapers/app/releases/latest/download/latest.json`
- Public key in `tauri.conf.json`, private key in GitHub Actions secrets
- Frontend can override endpoint for pre-release channel

## Key Config

- `tauri.conf.json` > `additionalBrowserArgs`: `--disable-features=CalculateNativeWinOcclusion` (prevents Chromium from pausing when behind other windows)
- `frontendDist` / `devUrl`: `https://dev.mywallpaper.online` (remote frontend)
- Window: fullscreen, no decorations, transparent, skip taskbar, not focusable

## Coding Guidelines

- **Error handling**: `Result<T, String>` for commands, `.expect()` only in `main.rs`
- **Platform code**: Use `#[cfg(target_os = "...")]` guards, not runtime checks
- **Comments**: French inline comments are OK (codebase convention), English for doc comments
- **Unsafe**: Required for Win32 API, MSAA — minimize scope, document why
