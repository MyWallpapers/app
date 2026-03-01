# Master Audit: MyWallpaper vs Seelen UI — Complete Reference

*Generated 2026-02-25*

---

## Table of Contents

1. [Who Does It Better — Summary Table](#1-who-does-it-better)
2. [Techniques & Approaches — Deep Comparison](#2-techniques--approaches)
3. [Feature Comparison Matrix](#3-feature-comparison-matrix)
4. [Improvement List — Filtered for MyWallpaper's Vision](#4-improvements-for-mywallpaper)
5. [Architecture & Best Practices Comparison](#5-architecture--best-practices)
6. [Tools & Technologies](#6-tools--technologies)
7. [Strengths & Weaknesses](#7-strengths--weaknesses)

---

## 1. Who Does It Better

| Area | Winner | Why |
|---|---|---|
| **WebView injection** | **Tie** | Both use 0x052C to Progman. MW has better style stripping + DWM border kill. Seelen has Win11 24H2 raised-desktop fallback path |
| **Mouse click handling** | **MyWallpaper** | Zero-alloc WH_MOUSE_LL hook + MSAA icon detection + COM-level forwarding via custom wry fork. Seelen doesn't need it (wallpaper doesn't intercept clicks) |
| **System data access** | **Seelen UI** | Battery, CPU, memory, disk, network, Bluetooth, media playback, notifications, power mode. MW only exposes OS/arch/version |
| **Plugin/widget system** | **Seelen UI** | 4 loader types, permissions, liveness probes, hot-reload, declarative settings schema. MW has nothing locally |
| **Platform/cloud addons** | **MyWallpaper** | SDK + sandboxed iframe canvas runtime + addon publishing. Seelen has no cloud |
| **Theming** | **Seelen UI** | SCSS compiler (grass crate), hot-reload, per-widget scoping. MW has nothing locally |
| **Multi-process architecture** | **Seelen UI** | 3-process (GUI + service + hook DLL), service survives app crash. MW is single-process |
| **Window management** | **Seelen UI** | Full tiling WM + virtual desktops + app launcher. MW doesn't compete here |
| **Update security** | **MyWallpaper** | Endpoint validation, downgrade protection, SSRF prevention. Seelen has empty signing key |
| **Desktop icon toggle** | **MyWallpaper** | ShowWindow + atomic safety flag + double-restore prevention. Seelen never hides icons |
| **Auto-start** | **Seelen UI** | Task Scheduler via COM with elevation detection + MSIX restart. MW uses basic plugin |
| **IPC design** | **Seelen UI** | Named pipes with token auth + hook DLL injection. MW is in-process only |
| **i18n** | **Seelen UI** | 70+ languages via Crowdin + rust-i18n. MW relies on remote frontend |
| **Performance (hook hot path)** | **MyWallpaper** | Zero-allocation, stack UTF-16 buffers, HWND caching, cached metrics |
| **Security overall** | **MyWallpaper** | SSRF prevention, updater hardening, deep-link validation, CSP, capabilities. Seelen broader but less deep |
| **Code conciseness** | **MyWallpaper** | 6 files ~1000 LOC vs massive codebase. Focused and clean |
| **Feature breadth** | **Seelen UI** | 20+ features vs 1 core feature — different scope entirely |
| **Error handling** | **Seelen UI** | AppError with backtrace + 25 From impls. MW uses Result<T, String> |
| **Type safety cross-lang** | **Seelen UI** | ts-rs + schemars auto-generates TS types. MW only has 2 typeshare types |
| **CI/CD breadth** | **Seelen UI** | Nightly builds + MSIX Store + Winget + multi-arch. MW has single workflow |
| **Testing** | **MyWallpaper Platform** | 218 integration tests + load tests + E2E. Desktop has 23, Seelen has minimal |
| **Observability (platform)** | **MyWallpaper** | Prometheus + Grafana + Jaeger + Loki. Seelen has no cloud |

---

## 2. Techniques & Approaches

### 2.1 WebView Injection (Behind Desktop Icons)

| Aspect | MyWallpaper | Seelen UI |
|--------|-------------|-----------|
| Progman message | `SendMessageTimeoutW(Progman, 0x052C)` — sync, 1s timeout | `PostMessageW(Progman, 0x052C)` — async fire-and-forget |
| API encoding | `FindWindowW` (Unicode) | `FindWindowA` (ANSI) |
| Win11 24H2 detection | SHELLDLL_DefView as child of Progman | Same + checks `WS_EX_NOREDIRECTIONBITMAP` on Progman |
| Raised desktop | Falls back to Progman as parent | Explicit path: parent to Progman, position below DefView |
| Style stripping | 11+ styles removed + DWM border kill + WM_NCCALCSIZE subclass | Minimal (WS_CHILDWINDOW + remove 3 EX styles) |
| WebView2 integration | Custom wry fork: `send_mouse_input_raw()` via composition controller | Standard Tauri WebviewWindow |
| Z-order | `SWP_NOZORDER` (inherits from parent) | `HWND_BOTTOM` explicit + DefView-relative |
| Multi-monitor | `EnumDisplayMonitors` bounding rectangle | Per-monitor widget instances |

### 2.2 Mouse Click Handling

| Aspect | MyWallpaper | Seelen UI |
|--------|-------------|-----------|
| Hook type | `WH_MOUSE_LL` (global low-level mouse hook) | `SetWinEventHook` (window events, not mouse) |
| Icon detection | MSAA `AccessibleObjectFromPoint` → `ROLE_SYSTEM_LISTITEM (34)` | Not needed |
| Click forwarding | Dual: COM via wry fork + synthesized messages to SysListView32 | None (wallpaper doesn't intercept clicks) |
| Hot path | Zero-alloc, stack UTF-16, HWND caching, cached metrics | 10fps mouse position polling |
| Double-click | Manual synthesis (timestamp + distance within system metrics) | N/A |
| Thread model | Dispatch window with packed WPARAM/LPARAM + COINIT_APARTMENTTHREADED | Crossbeam channels |
| Drag support | DRAG_VK atomic tracking, pre-sync cursor position | N/A |

### 2.3 System Data Access

| Data | MyWallpaper | Seelen UI |
|------|-------------|-----------|
| OS/arch/version | `os_info` crate | `sysinfo` crate |
| CPU cores/usage | **No** | `sysinfo::System::refresh_cpu_all()` — polled every 1s |
| Memory/RAM | **No** | `sysinfo::System::refresh_memory()` — total, free, swap |
| Disk info | **No** | `sysinfo::Disks::refresh()` — name, fs, total/available, read/written |
| Network stats | **No** | `sysinfo::Networks::refresh()` + `INetworkListManager` COM + native WiFi API |
| Battery level | **No** | `battery` crate (level, state, health, cycles, temp, energy rate) |
| Power mode | **No** | `EffectivePowerMode` (BatterySaver → GameMode) |
| Media playback | **No** | WinRT `GlobalSystemMediaTransportControlsSessionManager` (play/pause/skip/metadata) |
| Bluetooth | **No** | WinRT Bluetooth APIs (device enum, class parsing, pairing) |
| Notifications | **No** | `UI_Notifications_Management` toast interception |
| WiFi scanning | **No** | `netsh wlan` + native WiFi API (SSID, signal, secured) |
| Monitor brightness | **No** | WMI-based brightness query |
| Accent colors | **No** | Windows UI settings |

### 2.4 Plugin / Widget System

| Aspect | MyWallpaper | Seelen UI |
|--------|-------------|-----------|
| Local plugins | **None** — remote frontend only | 4 loader types (Legacy/React/Svelte/ThirdParty) |
| Cloud addons | SDK (`@mywallpaper/sdk-react`) + sandboxed canvas iframe | N/A (no cloud) |
| Permission model | iframe sandbox + postMessage | Per-widget dialog + declared capabilities + persisted to JSON |
| Hot-reload | No (remote deploy) | File watcher on widget resources |
| Liveness monitoring | **No** | Ping/pong every 5s, auto-reload after 5 failures |
| Settings schema | No | Declarative JSON → auto-generated UI (Switch, Select, Input, Range, Color) |
| Data isolation | N/A | Per-widget sandboxed data files in `{app_data}/data/{widget}/` |
| Instance modes | N/A | Single, Multiple (UUID), ReplicaByMonitor |

### 2.5 Theming

| Aspect | MyWallpaper | Seelen UI |
|--------|-------------|-----------|
| Engine | None locally | `grass` crate compiles SCSS/SASS at load time |
| Format | N/A | CSS / SCSS / SASS + CSS variables |
| Hot-reload | No | `notify-debouncer-full` file watcher |
| Scope | N/A | Per-widget + shared global styles |
| Settings | N/A | Declarative theme settings (switch, select, color, range) |

### 2.6 IPC / Multi-Process

| Aspect | MyWallpaper | Seelen UI |
|--------|-------------|-----------|
| Process count | 1 (Tauri only) | 3 (GUI + service + hook DLL) |
| IPC mechanism | In-process PostMessageW | Named pipes (`interprocess` crate) + WM_COPYDATA |
| Auth | N/A | Compile-time token for service pipe |
| Hook DLL | None | `WH_CALLWNDPROC` injected into Explorer for tray interception |
| Service | None | `slu-service` — survives app lifecycle, handles hotkeys + taskbar |
| Tauri commands | 7 | 50+ |
| Frontend source | Remote URL | Local build (React + Svelte) |
| Global Tauri | `withGlobalTauri: true` | `withGlobalTauri: false` |
| Type generation | `typeshare` (2 types) | `ts-rs` + `schemars` (full coverage) |
| Events | ~4 ad-hoc events | `SeelenEvent` enum with 40+ variants |

### 2.7 Update Mechanism

| Aspect | MyWallpaper | Seelen UI |
|--------|-------------|-----------|
| Plugin | `tauri_plugin_updater` | `tauri_plugin_updater` |
| Endpoint validation | Whitelisted: HTTPS + github.com/MyWallpapers/client | No validation |
| Downgrade protection | Semver comparison rejects older | None |
| SSRF prevention | Private IP blocking on OAuth URLs | None |
| Signing | Minisign (key in config) | Empty pubkey |
| Install mode | NSIS passive | NSIS |

### 2.8 Auto-Start & System Integration

| Aspect | MyWallpaper | Seelen UI |
|--------|-------------|-----------|
| Auto-start | `tauri_plugin_autostart` + LaunchAgent | Task Scheduler via COM + service IPC |
| Single instance | `tauri_plugin_single_instance` | Named mutex + self-pipe to existing instance |
| Elevation handling | None | Detects elevation, restarts as interactive user |
| MSIX awareness | None | Detects MSIX install, restarts as APPX |
| Tray | 1 item (Quit) + left-click show | Full tray replacement via DLL injection |
| Deep links | `mywallpaper://` with action whitelist | `seelen-ui.uri://` |

### 2.9 Desktop Icons

| Aspect | MyWallpaper | Seelen UI |
|--------|-------------|-----------|
| Method | `ShowWindow(SW_HIDE/SW_SHOW)` on SysListView32 | Never hidden — wallpaper sits behind |
| Safety | AtomicBool `ICONS_RESTORED` prevents double-restore | N/A |
| Cleanup | Runs on ExitRequested + Exit + tray quit | N/A |

---

## 3. Feature Comparison Matrix

| Feature | Seelen UI | MW Desktop | MW Platform |
|---------|-----------|------------|-------------|
| Wallpaper Injection (WorkerW/Progman) | Yes | Yes | N/A |
| Mouse Click Forwarding | N/A (not needed) | WH_MOUSE_LL + MSAA + COM | N/A |
| Multi-Monitor Support | Full (per-monitor widgets) | Bounding rect only | N/A |
| Tiling Window Manager | Yes (BSP-style tree) | No | No |
| Dock/Taskbar Replacement | Yes (macOS-style) | No | No |
| Fancy Toolbar (Menu Bar) | Yes (AppBar with plugins) | No | No |
| App Launcher | Yes (Rofi-style, fuzzy) | No | No |
| Virtual Desktops | Custom implementation | No | No |
| System Tray Replacement | Yes (DLL injection) | No | No |
| Task Switcher (Alt+Tab) | Custom replacement | No | No |
| Media Controls | Full (play/pause/skip) | No | No |
| Bluetooth Management | Full (WinRT APIs) | No | No |
| Notifications | Custom interception | No | No |
| Widget/Plugin System | Yes (4 loaders, hot-reload) | No | Yes (SDK + canvas sandbox) |
| Theme System | Yes (SCSS, hot-reload) | No | N/A (web-based) |
| Auto-Updater | Tauri updater | Tauri updater + endpoint validation + downgrade protection | N/A |
| Auto-Start | Task Scheduler via COM | tauri_plugin_autostart | N/A |
| Deep Linking | seelen-ui.uri:// | mywallpaper:// + action whitelist | OAuth callbacks |
| Single Instance | Named mutex | tauri_plugin_single_instance | N/A |
| OAuth Integration | No | Desktop OAuth with polling | Full (Google, GitHub) |
| Content Moderation | No | No | Yes (reviews, reports, trust) |
| Search | No | No | Yes (Meilisearch, fuzzy) |
| User Accounts | No | No | Yes (JWT, OAuth, profiles) |
| Content Upload/Share | No | No | Yes |
| Developer Portal | No | No | Yes (addon publishing) |
| Cloud Sync | No | No | Yes |
| ML Categorization | No | No | Yes (K-Means clustering) |
| i18n | 70+ languages | No (frontend-handled) | Yes (i18next) |
| Discord RPC | Yes | No | No |
| macOS Support | No (Windows-only) | Referenced but not implemented | Web (cross-platform) |
| Desktop Icon Toggle | No | Yes (ShowWindow) | N/A |
| Wallpaper per Workspace | Yes | No | N/A |
| Battery/CPU/Media data | Yes (full) | No (OS version only) | N/A |

---

## 4. Improvements for MyWallpaper

**Philosophy**: Only improvements that fit MyWallpaper's vision — a **single canvas wallpaper app** with a cloud platform. NOT trying to become a Seelen UI clone. No tiling WM, no taskbar replacement, no app launcher.

### Critical — Do First

| # | What | Why | How | Effort |
|---|------|-----|-----|--------|
| C1 | Structured error types | `Result<T, String>` loses all context for debugging. Seelen has `AppError` + backtrace | `thiserror` crate, `AppError` enum with variants per failure domain, `From` impls | 2-3 days |
| C2 | Session-aware mouse hook | Hook runs when screen is locked = wasted CPU. Seelen filters via `WM_WTSSESSION_CHANGE` | `WTSRegisterSessionNotification` on dispatch window, `IS_SESSION_ACTIVE` atomic early-return | 1 day |
| C3 | Resolve macOS situation | Docs describe macOS support that doesn't exist in code. False expectations | Either implement or remove all macOS references from config/docs | 1 day (cleanup) |

### High — Important Next

| # | What | Why | How | Effort |
|---|------|-----|-----|--------|
| H1 | Expose system data to canvas | MW canvas widgets have zero OS data (battery, CPU, media, network). Seelen exposes everything. This is the biggest gap for a "do everything" canvas | Add `sysinfo` + `battery` crates. New IPC commands: `get_battery_info`, `get_cpu_info`, `get_memory_info`, `get_media_info`. Poll every 1-5s, emit events to frontend | 1-2 weeks |
| H2 | WebView liveness monitoring | WebView could crash silently = black desktop, no recovery. Seelen has ping/pong every 5s | JS `setInterval` heartbeat → Tauri command. Backend timeout → reload WebView | 1-2 days |
| H3 | Zombie window detection | Cached HWNDs go stale if Explorer restarts. No recovery mechanism | Periodic `IsWindow()` check every 30s. If invalid → re-run `detect_desktop()` + re-inject | 2-3 days |
| H4 | Cross-language type generation | Only 2 types use typeshare. Frontend can drift. Seelen uses ts-rs + schemars for everything | Add `#[typeshare]` to all IPC types, publish as npm artifact, consume in remote frontend | 2-3 days |
| H5 | Per-monitor wallpaper | Single bounding rect stretches wallpaper across monitors. Seelen does per-monitor widgets | Enumerate monitors individually, create per-monitor WebView or CSS viewport regions | 2-3 weeks |
| H6 | Conventional commits + changelog | No commit convention enforcement. Seelen uses commitlint + Lefthook | Add commitlint + Husky. Add release-please or standard-version for changelogs | 1-2 days |

### Medium — Good to Have

| # | What | Why | How | Effort |
|---|------|-----|-----|--------|
| M1 | Media playback forwarding | Seelen exposes full media control (play/pause/skip/metadata) to widgets. Canvas widgets could show now-playing | WinRT `GlobalSystemMediaTransportControlsSessionManager`. New commands: `media_play_pause`, `media_next`, `media_prev`, `get_media_info` | 3-5 days |
| M2 | Nightly/pre-release channel | Only manual production releases. Seelen has automated nightly builds | `nightly.yml` workflow on push, timestamped versions, separate `nightly.json` manifest | 3-5 days |
| M3 | Structured event system | Ad-hoc Tauri events + Win32 messages don't scale. Seelen has `event_manager!` macro with 40+ typed events | `AppEvent` enum + `tokio::sync::broadcast` channel. Start simple, grow as needed | 3-5 days |
| M4 | `cargo audit` in CI | Desktop CI has no dependency security scanning. Platform has weekly Trivy | Add `cargo audit` + `cargo deny` step to release workflow | 1 day |
| M5 | Bundle integrity | Seelen verifies WebView2 runtime checksums. MW trusts whatever is installed | WebView2 runtime DLL checksum at startup + SRI for remote frontend critical scripts | 3-5 days |
| M6 | Multi-arch CI | Only builds x86_64 Windows. Seelen builds x86_64 + aarch64 | Add aarch64-pc-windows-msvc to build matrix | 2-3 days |
| M7 | MSIX Store distribution | Only GitHub Releases. Seelen has Microsoft Store + Winget. Store eliminates SmartScreen warnings | Partner Center account + MSIX bundle target + signing + Winget manifest | 1-2 weeks |

### Low — Future Consideration

| # | What | Why | How | Effort |
|---|------|-----|-----|--------|
| L1 | Bluetooth data for widgets | Seelen exposes full Bluetooth enumeration. Niche but useful for system-info canvas widgets | WinRT `Devices::Bluetooth` + `Devices::Enumeration` | 3-5 days |
| L2 | WiFi/network data for widgets | Seelen exposes WiFi scanning, network adapters, connectivity status | `sysinfo::Networks` + `INetworkListManager` COM | 3-5 days |
| L3 | Discord Rich Presence | Seelen has it. Low-cost community/marketing feature | `discord-rich-presence` crate. Show current wallpaper name | 1-2 days |
| L4 | i18n for desktop strings | Tray menu, errors, update notifications are English-only. Seelen has 70+ languages | `rust-i18n` crate for backend strings. Start with 5-10 languages | 1 week |
| L5 | Debounced state persistence | Seelen debounces virtual desktop state saves. MW will need this when it has local state | `tokio::time::sleep` debouncer when persistent state is added | 1 day |
| L6 | Reduce wry fork burden | Custom fork with version mismatch (windows 0.58 vs 0.61). Every upstream update = manual merge | Upstream PR to wry or create decoupled `wry-extensions` crate | 1-2 weeks |

### What NOT to Implement (Seelen features that don't fit MW's vision)

| Feature | Why Skip |
|---------|----------|
| Tiling Window Manager | Not a desktop environment. Different product category |
| Taskbar/Dock replacement | Same — MW is a wallpaper app, not a shell replacement |
| System Tray replacement | Requires DLL injection into Explorer. Huge attack surface, not related to wallpapers |
| App Launcher | Out of scope |
| Virtual Desktops | Out of scope |
| Alt+Tab replacement | Out of scope |
| Notification interception | Out of scope |
| Local frontend build (Preact/Svelte) | MW's remote frontend strategy is intentional and correct — enables rapid iteration without app updates |
| Windows Service process | Over-engineering for a wallpaper app. Single process is simpler and sufficient |
| 3-process architecture | Same — adds complexity MW doesn't need |
| Hook DLL injection | Same — too invasive for a wallpaper app |

---

## 5. Architecture & Best Practices

| Dimension | Seelen UI | MW Desktop | MW Platform |
|-----------|-----------|------------|-------------|
| Pattern | Monolithic desktop + service + hook DLL | Thin native shell, remote frontend | Microservices (5 services, 9 libs) |
| Process model | 3 processes | 1 process | Multiple K8s containers |
| Frontend | Local (Preact + Svelte, esbuild) | Remote (CDN) | React 19 + Vite 7 SPA |
| Database | File-based (YAML/JSON) | None | ScyllaDB + Redis + Meilisearch |
| State (backend) | ArcSwap + file watchers | Atomics (minimal) | CQRS + two-tier cache |
| State (frontend) | Preact signals + Svelte runes | N/A (remote) | Zustand + TanStack Query |
| Deployment | MSIX + Winget + GitHub + .exe | GitHub Releases (MSI + NSIS) | K8s (ArgoCD GitOps) |

| Practice | Seelen UI | MW Desktop | MW Platform |
|----------|-----------|------------|-------------|
| Error types | Excellent (AppError + backtrace) | Weak (String) | Good (domain-core) |
| Type safety cross-lang | Excellent (ts-rs + schemars) | Minimal (2 types) | Good (typeshare + graphql-codegen) |
| Lint strictness | Clippy -D + deno lint | Clippy -D | Clippy -D + ESLint strict |
| Test coverage | Minimal (empty npm test) | 23 unit tests | 218 integration + load + E2E |
| Security scanning | Not documented | .security-hardening/ exists | Trivy + cargo audit weekly |
| Commit conventions | commitlint + Lefthook | Not enforced | Not documented |
| CI/CD maturity | CI + Release + Nightly + MSIX + Winget | Validate + Build + Release | Fast/Standard/Release + GitOps |
| Observability | Tauri plugin-log | Tauri plugin-log (3 targets) | Prometheus + Grafana + Jaeger + Loki |
| Input validation | Path traversal protection | URL + deep-link validation, SSRF prevention | validator + Zod + ammonia |

---

## 6. Tools & Technologies

| Technology | Seelen UI | MW Desktop | MW Platform |
|------------|-----------|------------|-------------|
| Core framework | Tauri v2.10.2 | Tauri v2.0 | Axum |
| Async runtime | Tokio | Tokio (via Tauri) | Tokio |
| Win32 bindings | windows-rs 0.59 (170+ features) | windows 0.58 (12 features) | N/A |
| WebView | wry (stock) | wry 0.53.5 (custom fork) | N/A |
| Frontend framework | Preact + Svelte 5 | Remote (React 19) | React 19 |
| Build (frontend) | esbuild | N/A | Vite 7 |
| Build (Rust) | Cargo + LLD | Cargo | Cargo + mold + sccache |
| CSS | CSS Modules + Variables | N/A | TailwindCSS 3 |
| UI library | Ant Design | N/A | Custom |
| State (frontend) | Preact signals + Svelte runes | N/A | Zustand + TanStack Query |
| State (backend) | ArcSwap + file watchers | Atomics | CQRS + two-tier cache |
| IPC | Named pipes (interprocess) | Win32 messages | RabbitMQ |
| Database | YAML/JSON files | None | ScyllaDB + Redis |
| Search | N/A | N/A | Meilisearch |
| Type generation | ts-rs + schemars | typeshare | typeshare + graphql-codegen |
| Validation | N/A | url crate | validator + Zod |
| i18n | rust-i18n + i18next (70+ langs) | None | i18next |
| Serialization | serde (JSON + YAML) | serde (JSON) | serde (JSON) |
| HTTP client | reqwest | N/A | reqwest |
| Linting (TS) | deno lint + deno fmt | N/A | ESLint 9 + Prettier |
| Linting (Rust) | clippy + fmt | clippy + fmt | clippy + fmt |
| Testing | cargo test (CI) | cargo test (23 tests) | nextest + Vitest + Playwright + k6 |
| CI/CD | GitHub Actions (4 workflows) | GitHub Actions (1 workflow) | GitHub Actions (3) + ArgoCD |
| Distribution | MSIX Store + Winget + GitHub | GitHub Releases | Kubernetes (K3s) |
| Code signing | SignPath (MSIX) | Minisign (updater) | N/A |
| Monitoring | N/A | N/A | Prometheus + Grafana + Jaeger + Loki |
| Secrets | N/A | GitHub Actions secrets | Sealed Secrets |
| ML | N/A | N/A | linfa (K-Means) |

---

## 7. Strengths & Weaknesses

### Seelen UI

**Strengths:**
1. Unmatched Windows integration depth — DLL injection, WinRT Bluetooth/media, custom virtual desktops
2. Mature extensibility — 4 widget loaders, permissions, liveness, hot-reload, declarative settings
3. Excellent error handling — AppError + backtrace + 25 From impls
4. Strong cross-language type safety — ts-rs + schemars
5. Comprehensive CI/CD — Nightly + MSIX Store + Winget + multi-arch
6. 70+ languages via Crowdin
7. Custom event system — event_manager! macro, prioritized pub/sub

**Weaknesses:**
1. Windows-only — zero cross-platform capability
2. Dual framework complexity (Preact + Svelte)
3. Nightly Rust dependency (unstable features)
4. Minimal testing
5. Unsigned exe installer (SmartScreen warnings)
6. 90+ crate dependency surface
7. No cloud component — all state is local, no sync/sharing/community

### MyWallpaper Platform

**Strengths:**
1. Production-grade infrastructure — K8s + GitOps + sealed secrets + observability
2. Comprehensive testing — 218 integration + load + E2E
3. Sophisticated caching — two-tier with stampede protection
4. Security-by-default — deny-all network policies, distroless, non-root, Trivy
5. Content platform features — moderation, search, ML categorization, addon SDK
6. CQRS architecture
7. Smart CI/CD — change detection, sccache, three build modes

**Weaknesses:**
1. No monorepo orchestration (no Turborepo/Nx)
2. Single-region deployment
3. ScyllaDB complexity (29 tables, heavy denormalization)
4. Missing commit conventions
5. No MSIX/Store distribution for desktop

### MyWallpaper Desktop

**Strengths:**
1. Excellent mouse hook engineering — zero-alloc, dispatch window, COM threading, process validation
2. Strong updater security — endpoint validation, downgrade prevention, TOCTOU protection
3. Code conciseness — 6 files, clean two-layer architecture
4. OAuth SSRF prevention — private IP blocking, IPv6 loopback, IPv4-mapped IPv6
5. Remote frontend pattern — rapid iteration without app updates
6. Chromium GPU optimization — well-tuned for animated wallpaper rendering
7. Safety mechanisms — atomic icon restoration, hook cleanup on all exit paths

**Weaknesses:**
1. String-based error handling — no backtraces, no type info
2. No extensibility — no widget, plugin, or theme system
3. Missing macOS implementation despite documentation
4. Limited test coverage — 23 unit tests only
5. Wry fork maintenance burden + version mismatch
6. No structured event system
7. Single-platform CI (Windows only despite macOS in config)
8. No session-aware hook filtering (wastes CPU when screen locked)
9. No commit conventions
10. No nightly/pre-release channel
11. Zero system data exposure (only OS version) — biggest feature gap

---

*End of master audit. Source files:*
- `/tmp/audit/01-seelen-ui.md` — Seelen UI deep analysis
- `/tmp/audit/02-mywallpaper-platform.md` — MyWallpaper Platform analysis
- `/tmp/audit/03-mywallpaper-desktop.md` — MyWallpaper Desktop analysis
- `/tmp/audit/04-comparative-audit.md` — Comparative synthesis
- `/tmp/audit/05-techniques-comparison.md` — Techniques deep-dive
