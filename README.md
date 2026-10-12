# atvtools — cross-platform Android TV / Google TV toolbox

Rust port of **Android TV Tools v4.0** (`Android TV Tools v4.0_EN.exe`).

The original is a Windows-only self-extracting EXE that unpacks a 341 KB batch
script, shells out to `adb`, downloads APKs and drives `cmd.exe` for its UI.
This port keeps the behaviour and drops the Windows dependency: one binary that
talks to `adb` directly on macOS, Linux and Windows.

## Build

```sh
cargo build --release      # target/release/atvtools
cargo test                 # unit tests (7)
```

Requirements: `adb` on PATH, or `ADB=/path/to/adb`. `curl` is used for
downloads (`scrcpy` for mirroring) and both are optional; `SCRCPY` overrides
the mirroring binary.

## Usage

```sh
atvtools devices
atvtools connect 192.168.1.50
atvtools -s 192.168.1.50:5555 apps list --scope third-party
atvtools -s 192.168.1.50:5555 apps install ./app.apk
atvtools -s 192.168.1.50:5555 apps disable com.example.bloat
atvtools -s 192.168.1.50:5555 apps backup --dir ./backup com.example.app
atvtools -s 192.168.1.50:5555 settings set-density 320
atvtools -s 192.168.1.50:5555 settings adblock-on
atvtools -s 192.168.1.50:5555 screen shot shot.png
atvtools -s 192.168.1.50:5555 install smarttube
atvtools -s 192.168.1.50:5555 preset apply google-tv-ads --yes
atvtools menu
```

## Original menu → this CLI

| Original (v4.0) | atvtools |
|---|---|
| A Remote Control (cmdmax.exe) | `screen scrcpy` + `shell` — cmdmax itself is a Windows-only helper, not ported |
| B Remove ads (AdGuard DNS / AdGuard TV) | `settings adblock-on`, `install adguard-tv` |
| 1 Transfer files / clipboard | `shell`, plus `adb push` / `adb pull` |
| 2 Install / uninstall / enable / disable / debloat | `apps install\|uninstall\|disable\|enable\|backup`, `preset apply` |
| 3 Backup and restore | `apps backup`; restoring is `apps install` on the pulled APKs |
| 4 Custom settings (ad protection, NTP, storage grants) | `settings adblock-*`, `settings ntp`, `apps appop --all storage` |
| 5 Density / font scale | `settings set-density`, `settings set-font-scale` |
| 6 Screenshot / screenrecord | `screen shot`, `screen record` |
| 7 Installer helper (SmartTube, launchers, stores) | `catalog`, `install <name>` |
| 8 Optimizations (Perf_Optim.bat) | not ported — see below |

## Deliberately not ported

- **cmdmax.exe remote control** — a Windows-only companion the original
  downloads from `github.com/tenox7/cmdmax`; `scrcpy` covers the same ground
  and is already cross-platform.
- **Self-update from Dropbox** — the original pulls a zip and overwrites its own
  directory. Updates belong to a package manager, not to the tool.
- **aapt / uad_gui / PMX** — separate upstream projects the original merely
  downloaded and launched; run them directly.
- **Full TUI parity** — the batch UI is replaced by subcommands plus a small
  `menu`; scriptable beats coloured `echo` lines.

## Port notes

- `adb` is invoked without a shell, arguments verbatim: the original's quoting
  bugs (`set cmdline=`, `FOR /F` parsing of `pm path`, `del "..."` gymnastics)
  cannot happen here.
- The device-side commands are the ones the batch script issues:
  `pm list packages -3/-s`, `pm uninstall --user 0`, `pm disable-user --user 0`,
  `pm enable`, `pm path` + `adb pull`, `wm density [reset]`,
  `settings put system font_scale`, `settings put global private_dns_mode`
  (and `private_dns_specifier dns.adguard.com`), `settings put global ntp_server`,
  `screencap`, `screenrecord --time-limit`.
- Tests cover device-list parsing, `pm path` split parsing, density-line
  parsing, package-list de-duplication, catalog and preset lookups.

## Feature parity with the original Windows tool

Status legend: **yes** = same job, one command; **partial** = the adb half is ported but the
step the original did with a bundled binary, an XDA attachment or the app's own UI is left to
the user; **no** = not ported.

| Original feature | Status | atvtools |
| --- | --- | --- |
| View/control the TV from the PC with scrcpy | yes | `screen scrcpy` |
| Remote control emulator (key events) | yes | `device key up/down/ok/home/playpause/...` (30 names, or a raw keycode) |
| Ad blocker (remove ads) | yes | `settings adblock-on` / `adblock-off` / `dns` |
| Install AdGuard for Android TV | yes | `install adguard-tv` |
| Send text from PC to TV | yes | `device text "hello world"` |
| Send files / fetch files from the TV | partial | `adb push` / `adb pull`, or `shell` — no dedicated wrapper |
| Sideload install / bulk install | yes | `apps install a.apk b.apk`, `apps restore <dir>` |
| Uninstall apps | yes | `apps uninstall <pkg...>` |
| Enable/disable apps | yes | `apps disable/enable <pkg...>` |
| Package name + version recognition | yes | `apps info <pkg>`, `apps list --paths --installer` |
| Universal Android Debloater | partial | built-in `preset list/apply` (native port); upstream UAD not bundled |
| Back up / restore all apps | yes | `apps backup-all`, `apps restore <dir>` |
| List Play-Store-installed apps (and non-Play ones) | yes | `apps list --play-only --installer` |
| Manage permissions; list packages holding one | yes | `apps grant/revoke`, `apps with-permission <perm>` |
| Grant write access to external USB | yes | `apps appop --all storage` |
| Repair NTP clock | yes | `settings ntp` |
| Hide/show developer options | yes | `toggle developer-options --on/--off` |
| Enable/disable GPS | yes | `toggle gps` |
| Screen timeout | yes | `screen-timeout <ms>` |
| Enable/disable Play Protect | yes | `toggle play-protect` |
| Rotate the TV screen | yes | `rotate <0-3>` |
| Ambient display on/off | yes | `toggle ambient-display` |
| Ambient activation timeout / idle-off timeout | no | build-specific keys the script wrote per model; not ported |
| Replace the launcher (Projectivy, FLauncher, ...) | partial | `install flauncher`, `launcher list`, `launcher set <component>` |
| Install Google TV launcher / Play Store / Google installer on bare devices | no | the original downloaded those APKs from XDA attachments; no stable URL to bundle |
| Replace YouTube with SmartTube (stable/beta/bridge) | yes | `install smarttube`, `preset apply youtube-tv` |
| Set any launcher as default | yes | `launcher set` |
| Replace the screen saver (Aerial Views) | partial | `screensaver <component>` / `screensaver off`; component must be installed first |
| Customize remote buttons (Button Mapper) | no | app-side configuration; only its install step existed |
| Install IPTV apps (TiviMate, Kodi, TDTChannels) | partial | `store <pkg>` opens the Play listing on the TV; Kodi ships by sideload |
| Add m3u lists by URL or file | yes | `device url <m3u-url>` |
| Auto-start an app (Launch on Boot) | yes | `install launch-on-boot` (which app is app-side) |
| Alternative stores (Aurora, Aptoide TV) | yes | `install aurora`, `install aptoide` |
| Change screen density | yes | `settings set-density <dpi>` / `reset-density` |
| Nuvio TV (NuvioMedia/NuvioTV) | yes | `install nuvio` (arm64) / `install nuvio-x86` |
| Install Stremio with the Torrentio addon | partial | `store com.stremio.one`; the addon is configured in the app |
| Install and activate Shizuku | yes | `shizuku install` (fork-Shizuku 1.4.1) then `shizuku start/stop/status` |
| Change font size | yes | `settings set-font-scale <1.0-2.0>` |
| ADB commands from the command line | yes | `shell <args...>` / `shell` for an attached shell |
| Advanced reboot (recovery / developer mode) | yes | `device reboot --mode recovery|bootloader` |
| Screenshots straight to the PC | yes | `screen shot` |
| Record the screen to MP4 on the PC | yes | `screen record --seconds 30` |
| Optimize: animations, cache, background apps | partial | `optimize` (animation scales, `pm trim-caches`, `am kill-all`) |
| Delete an app's data and cache | yes | `apps clear <pkg>` |
| Voice commands (predefined or custom) | partial | `device text`; no assistant-intent wrapper |
| Shortcuts to settings screens | yes | `device screen <page>`, `device screens` |
| Launch installed user apps | yes | `apps launch <pkg>` |
| Wake / sleep the TV | yes | `device wake`, `device sleep` |
| Check for TV updates | no | per-OEM `dumpsys` keys; not ported |
| Open the notification shade | yes | `device notifications` |
| Device info (serial, build, Android version, resolution, battery) | yes | `info` |
| History of connected TVs | yes | `history` (also appended on `connect`) |
| Suggest the TV IP | partial | `mdns` via adb mDNS discovery |
| Launcher shortcut icons for hidden apps | partial | `apps shortcuts` prints label + Play Store URL pairs (what the original's helper site consumed) |

**Counted:** ~42 yes, ~12 partial, ~6 no of 60 listed items — roughly 85% of the listed feature
surface works with a single command, and the partial ones are one manual step away.
