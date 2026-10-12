# atvtools — cross-platform Android TV / Google TV toolbox

Rust port of **Android TV Tools v4.0** (`Android TV Tools v4.0_EN.exe`).

The original is a Windows-only self-extracting EXE that unpacks a 341 KB batch
script, shells out to `adb`, downloads APKs and drives `cmd.exe` for its UI.
This port keeps the behaviour and drops the Windows dependency: one binary that
talks to `adb` directly on macOS, Linux and Windows.

## Build

```sh
cargo build --release      # target/release/atvtools
cargo test                 # unit tests (31)
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
atvtools                         # bare invocation = the interactive menu
atvtools menu --lang es

# added over the Windows tool
atvtools -s 192.168.1.50:5555 remote                 # keypad in this terminal
atvtools -s 192.168.1.50:5555 remote uudok           # scripted key sequence
atvtools -s 192.168.1.50:5555 transfer push ./a.mkv /sdcard/Download
atvtools -s 192.168.1.50:5555 transfer pull /sdcard/Download/list.m3u ./list.m3u
atvtools -s 192.168.1.50:5555 iptv add https://example.tld/spain.m3u
atvtools -s 192.168.1.50:5555 iptv open https://example.tld/spain.m3u
atvtools -s 192.168.1.50:5555 google --dir ~/Downloads/google-tv-apks
atvtools -s 192.168.1.50:5555 settings idle-timeout 900
atvtools -s 192.168.1.50:5555 settings ota --off
atvtools doctor                  # adb version, devices, internet, egress country, OTA
atvtools update                  # newer release?; update --beacon <url> reads the old Dropbox beacon
atvtools apk info ./app.apk      # label, version, launch activity (aapt2 on PATH, or AAPT=...)
```

## Original menu → this CLI

| Original (v4.0) | atvtools |
|---|---|
| A Remote Control (cmdmax.exe + RC_Emulator.bat) | `remote` (interactive keypad), `remote uudok` (scripted), `screen scrcpy`; `doctor` covers the console diagnostics |
| B Remove ads (AdGuard DNS / AdGuard TV) | `settings adblock-on`, `install adguard-tv` |
| 1 Transfer files / clipboard | `transfer push\|pull\|ls\|mkdir`, `device text` — no FTP app on the TV needed |
| 2 Install / uninstall / enable / disable / debloat | `apps install\|uninstall\|disable\|enable\|backup`, `preset apply` |
| 3 Backup and restore | `apps backup`; restoring is `apps install` on the pulled APKs |
| 4 Custom settings (ad protection, NTP, storage grants, OTA, idle timeout) | `settings adblock-*`, `settings ntp`, `apps appop --all storage`, `settings ota`, `settings idle-timeout` |
| 5 Density / font scale | `settings set-density`, `settings set-font-scale` |
| 6 Screenshot / screenrecord | `screen shot`, `screen record` |
| 7 Installer helper (SmartTube, launchers, stores, IPTV) | `catalog`, `install <name>`, `iptv install\|add\|open` |
| 8 Optimizations (Perf_Optim.bat) | `optimize` |

## Not ported as a bundled binary, and why

- **cmdmax.exe** — its job was resizing console windows on Windows. The keypad
  is now `remote` in any terminal, the device list is `devices`/`history`,
  the console diagnostics are `doctor`, and mirroring is `screen scrcpy`.
- **Self-overwriting Dropbox updater** — the original downloaded a zip and
  unpacked it over its own folder, which is how "v5.0" turned out to be an older
  build. `update` only reports; `update --beacon <url>` still reads the old
  `version;url;changelog` file.
- **uad_gui-windows.exe / PMX / Button Mapper configuration** — separate
  upstream apps with their own UI. Install them (`catalog`, `store`) and
  configure them there; the debloat presets are ported natively.
- **Windows ADB driver installer** — Windows-only. `doctor` reports the adb
  version and where to get current platform-tools instead.
- **Google installer / Play Store / Google TV launcher APKs** — the original
  pulled them from XDA attachments, which rot. `google --dir <folder>` installs
  the copies you supply in dependency order: installer, services, store, launcher.
- **Countries list attachment** — a text file about geo-blocked IPs. `doctor`
  prints the egress country, which is the actionable part.
- **2000 lines of coloured `echo`** — replaced by the ANSI `menu` (EN/ES) plus
  subcommands, so the same steps can be scripted.

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
  parsing, package-list de-duplication, catalog and preset lookups, permission
  scans, keycode/shorthand expansion, m3u name sanitising, aapt badging parsing,
  adb version detection, beacon parsing and the Google install order.

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
| Send files / fetch files from the TV | yes | `transfer push\|pull\|ls\|mkdir` (adb; no FTP app on the TV) |
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
| Ambient activation timeout / idle-off timeout | yes | `settings idle-timeout <seconds>` writes both keys and reads back what stuck |
| Replace the launcher (Projectivy, FLauncher, ...) | yes | `install projectivy`, `install flauncher`, `launcher list`, `launcher set <component>` |
| Install Google TV launcher / Play Store / Google installer on bare devices | yes | `google --dir <folder>` installs your copies in dependency order |
| Replace YouTube with SmartTube (stable/beta/bridge) | yes | `install smarttube`, `preset apply youtube-tv` |
| Set any launcher as default | yes | `launcher set` |
| Replace the screen saver (Aerial Views) | partial | `screensaver <component>` / `screensaver off`; component must be installed first |
| Customize remote buttons (Button Mapper) | partial | `store flar2.homebutton` installs it; the mapping is app-side |
| Install IPTV apps (TiviMate, Kodi, TDTChannels) | yes | `iptv install tivimate\|kodi\|tdtchannels\|vlc` |
| Add m3u lists by URL or file | yes | `iptv open <url>` (player app), `iptv add <url>` (copy into the TV), `iptv list` |
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
| Voice commands (predefined or custom) | yes | `voice <text>` fires the assistant key, then dictates |
| Shortcuts to settings screens | yes | `device screen <page>`, `device screens` |
| Remote control without cmdmax | yes | `remote` (interactive keypad), `remote uudok` (scripted) |
| APK labels and versions (on-device aapt) | yes | `apk info <file>`, `apk list <dir>` — read on the PC, nothing written to /data/local/tmp |
| Dropbox version beacon | yes | `update --beacon <url>`; plain `update` checks GitHub releases |
| ES/EN localisation (two EXEs in the original) | yes | `menu --lang es`, or press L inside the menu |
| Country list helper | yes | `doctor` prints the egress country |
| Windows ADB driver install | partial | `doctor` reports the adb version and the platform-tools hint; it does not install drivers |
| Launch installed user apps | yes | `apps launch <pkg>` |
| Wake / sleep the TV | yes | `device wake`, `device sleep` |
| Check for TV updates | partial | `settings ota [--off\|--on]` blocks/allows OTA; firmware checks stay per-OEM |
| Open the notification shade | yes | `device notifications` |
| Device info (serial, build, Android version, resolution, battery) | yes | `info` |
| History of connected TVs | yes | `history` (also appended on `connect`) |
| Suggest the TV IP | yes | `mdns` (adb mDNS discovery), `doctor` |
| Launcher shortcut icons for hidden apps | partial | `apps shortcuts` prints label + Play Store URL pairs (what the original's helper site consumed) |

**Counted:** 52 yes, 8 partial, 0 no of 60 listed items — roughly 87% of the listed feature surface works with a single command; the partial ones are one manual step away.
surface works with a single command, and the partial ones are one manual step away.
