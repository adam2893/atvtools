# Changelog

All notable changes to **atvtools** are documented in this file.

## [5.1.0] - 2026-11-22

### Added
- **Brand-Specific Debloat Presets:** Added `tcl-bloat`, `xiaomi-patchwall`, `hisense-bloat`, `sony-bravia`, `philips-bloat`, and `shield-bloat` presets plus `Detect TV Brand` auto-matching in **TV Cleanup**.
- **Drag-and-Drop APK Sideloading:** Drop `.apk` files anywhere onto the desktop GUI window to sideload, with an automatic Android 14+ shell fallback (`push` + `pm install -r -g`).
- **Storage & Cache Analyzer:** Added **Analyze Sizes (Top 15)** and **Trim Caches** (`pm trim-caches`) in the **Apps** tab.
- **One-Click Backup & Restore Snapshots:** Save and restore timestamped `.atvtools.toml` TV snapshots under `~/.atvtools/snapshots/`.
- **USB to Wi-Fi Switch & Wake-on-LAN:** Added **USB -> Wi-Fi (5555)** and **Wake-on-LAN** buttons in **Connect**.
- **Live Remote Key / Scancode Inspector:** Added **Inspect Remote Key (4s)** in **Remote & Screen** using `getevent -lt` for remapping TV remote buttons.
- **Full Private DNS Provider Picker:** Exposed AdGuard, Cloudflare, NextDNS (with profile ID), Quad9, and Google DNS in **Settings**.
- **Animation Speed & Screen Size Controls:** Added `0x Instant`, `0.5x Snappy`, `1.0x Stock` animation scale buttons and `wm size` read/reset in **Settings**.
- **Logs & Crash Viewer:** Added filterable `logcat` dump, recent crash/ANR viewer, and bugreport export in **Files & Catalog**.
- **Multi-TV Fleet Apply:** Added **Apply to all attached TVs** toggle in **TV Cleanup**.
- **Security Audit & ADB Lockdown:** Added Step 4 **Security Audit** (`ro.adb.secure`, `adb_enabled`, `adb_wifi_enabled`) and **Open Developer Options on TV** shortcut.
- **macOS App Icon:** CI now converts `assets/logo.png` into `AppIcon.icns` inside `atvtools.app`.

## [5.0.0] - 2026-11-22

### Added
- **Non-Blocking Anti-Slop GUI:** Redesigned `egui` interface with cool-slate dark theme, locked `#3B82F6` accent, left sidebar navigation, and background worker threads.
- **UAD-ng Inspired Typed ADB Layer:** Added `ACommand` / `ShellCommand` / `PmCommand` builders, validated `PackageId`, `UserInfo` parsing, Windows `CREATE_NO_WINDOW`, and OEM-friendly error translation.
- **CleanYourTV Safety & Privacy Features:**
  - Core Android & MediaTek/CVTE middleware brick protection.
  - `setupwraith` Wi-Fi ownership guard (`dumpsys wifi`).
  - Last usable `HOME` launcher guard and factory-menu launch block.
  - Privacy/ACR/telemetry audit (`atvtools privacy`), OTA drift check/fix (`atvtools drift`), and `atvtools home <pkg>`.
- **New Catalog Additions:** Added **tvOS Launcher** (`techgenius1/tvOS-Launcher`) and **Stellar** (`roro2239/Stellar`) as an optional alternative to `fork-Shizuku`.
- **Branding & Packaging:** Added `assets/logo.png`, *"Take back your TV."* tagline, and double-clickable `atvtools.app` / `atvtools-gui.bat` release bundles.

## [4.1.0] - Initial Cross-Platform Rust Release
- Cross-platform CLI, terminal menu (EN/ES), and initial `egui` desktop GUI.
- UAD database integration, presets, undo journal, profiles, fleet sync, IPTV helpers, and `scrcpy` support.
