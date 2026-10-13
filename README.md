# atvtools — cross-platform Android TV / Google TV toolbox

Feature set modelled on **Android TV Tools v4.0** (`Android TV Tools v4.0_EN.exe`),
reimplemented from scratch in Rust. Nothing of the original ships here: no batch
script, no `cmdmax.exe`, no XDA attachments.

The original is a Windows-only self-extracting EXE that unpacks a 341 KB batch
script, shells out to `adb`, downloads APKs and drives `cmd.exe` for its UI.
This implementation keeps the behaviour and drops the Windows dependency: one
binary that talks to `adb` directly on macOS, Linux and Windows.

## Download

Prebuilt binaries are on the [releases page](https://github.com/adam2893/atvtools/releases):
macOS (one universal binary, Intel and Apple silicon), Linux x86-64 and Windows x86-64.
Unpack it and put `atvtools` somewhere on your PATH — it is the CLI and the GUI in one file.

The Linux build is compiled on Ubuntu 22.04, so it wants glibc 2.35 or newer
(Ubuntu 22.04+, Debian 12+, Fedora 36+) plus the X11/Wayland libraries a window
needs; on anything older, or on musl, `cargo install --git` is the way. The macOS
binary is unsigned and not notarised — the first launch needs right-click → Open,
or `xattr -d com.apple.quarantine atvtools`. Only x86-64 Linux is built today, and
the Windows binary is built and packaged by CI but has not been run here.

## Build

```sh
cargo build --release      # target/release/atvtools (CLI + GUI)
cargo test                 # unit tests (88)
```

Tagging (`git tag v4.1.0 && git push --tags`) or running the *release* workflow by hand
builds all three platforms on GitHub runners and attaches the archives to a new release
(`.github/workflows/release.yml`). The tag names the build: the workflow writes it into
`Cargo.toml` before compiling, so `atvtools --version` matches the release.

Requirements: `adb` on PATH, or `ADB=/path/to/adb`. `curl` is used for
downloads (`scrcpy` for mirroring) and both are optional; `SCRCPY` overrides
the mirroring binary.

Two front ends, one binary: `atvtools gui` opens a native window, bare
`atvtools` opens the terminal menu, and every command below is also a plain
subcommand for scripting.

## Usage

```sh
atvtools devices                 # add --json for one JSON object per TV
atvtools connect                 # scan this network and list the TVs that answered
atvtools connect 192.168.1.50    # or connect straight to a known address
atvtools pair                    # list the pairing ports on this network (Android 11+)
atvtools pair 192.168.1.50:37123 --code 123456   # wireless debugging pairing
atvtools -s 192.168.1.50:5555 apps list --scope third-party
atvtools -s 192.168.1.50:5555 apps install ./app.apk
atvtools -s 192.168.1.50:5555 apps disable com.example.bloat
atvtools -s 192.168.1.50:5555 apps backup --dir ./backup com.example.app
atvtools -s 192.168.1.50:5555 settings set-density 320
atvtools -s 192.168.1.50:5555 settings adblock-on
atvtools -s 192.168.1.50:5555 screen shot shot.png
atvtools -s 192.168.1.50:5555 install smarttube
atvtools -s 192.168.1.50:5555 preset apply google-tv-ads --yes
atvtools -s 192.168.1.50:5555 install smarttube --all-devices   # every attached TV
atvtools -s 192.168.1.50:5555 apps install ./shizuku.apk --replace   # the installed copy has a different signature
atvtools -s 192.168.1.50:5555 preset undo google-tv-ads   # re-enable what a preset disabled
atvtools                         # bare invocation = the interactive menu
atvtools menu --lang es
atvtools gui                     # native window: Connect / Apps / Files / Debloat / Settings / Screen / Remote / Helpers

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
atvtools doctor --fix            # restart the adb server, put a USB TV on port 5555
atvtools update                  # newer release?; update --beacon <url> reads the old Dropbox beacon
atvtools apk info ./app.apk      # package, label, version, launch activity (read in-process; AAPT=<bin> to differ)
atvtools uad list                # installed packages the UAD database flags as safe to remove
atvtools uad apply --level recommended --yes   # disable all of them at once
atvtools uad info com.google.android.youtube  # what the database says about one package
atvtools uad refresh             # re-download the database into ~/.atvtools
atvtools uad gui                 # run the upstream UAD window (downloaded once to ~/.atvtools/uad)

# new in this batch: what changed, what is out of date, and undoing a debloat
atvtools -s 192.168.1.50:5555 apps size --top 20     # biggest packages, apk + data
atvtools -s 192.168.1.50:5555 apps outdated          # installed version vs the catalog
atvtools -s 192.168.1.50:5555 apps upgrade smarttube # download and install the newer build
atvtools -s 192.168.1.50:5555 apps apk com.example.app --dir ./apk   # pull an installed APK
atvtools -s 192.168.1.50:5555 apps list --disabled   # only the packages that are turned off
atvtools -s 192.168.1.50:5555 undo                   # re-enable the last disabled batch
atvtools -s 192.168.1.50:5555 undo --list            # every batch still in the journal
atvtools -s 192.168.1.50:5555 logs --lines 200 --grep wifi
atvtools -s 192.168.1.50:5555 logs --follow          # live stream, ctrl-c to stop
atvtools -s 192.168.1.50:5555 crash                  # recent crashes/ANRs out of the dropbox
atvtools -s 192.168.1.50:5555 bugreport --dir ./br   # the device writes the archive
atvtools -s 192.168.1.50:5555 device wake --lan      # Wake-on-LAN magic packet to the TV MAC
atvtools -s 192.168.1.50:5555 remote --script macros/start.txt   # keys, delay, text, url
atvtools -s 192.168.1.50:5555 remote uu --repeat 3   # the same sequence three times
atvtools -s 192.168.1.50:5555 remote --serve         # browser D-pad for the phone (0.0.0.0:8080)
atvtools -s 192.168.1.50:5555 settings dns cloudflare     # or nextdns --id <profile>
atvtools -s 192.168.1.50:5555 apply -f tv.atvtools.toml --dry-run
atvtools preset export -f tv.atvtools.toml           # dump this TV into a profile
atvtools fleet status                                # every attached TV at a glance
atvtools fleet sync -f tv.atvtools.toml --yes        # the same profile on all of them
atvtools -s 192.168.1.50:5555 screen shot            # shot.png; --all-devices writes shot-<serial>.png
atvtools -s 192.168.1.50:5555 transfer push ./movies --dir   # a whole tree
atvtools --wait -s 192.168.1.50:5555 install smarttube       # wait for the TV to answer first
```

A profile is a small TOML file (`.atvtools.toml`): an optional `[device] serial`,
`[settings]` density / font scale / private DNS / screen-off timeout, and
`[apps] install`, `disable`, `enable` lists. `apply` prints the plan first and
asks before changing anything; `preset export` writes the file from the TV you
are connected to. Every disable also lands in `~/.atvtools/undo`, so `undo` (or
`preset undo <name>`) can put it back.

## GUI

`atvtools gui` (or `--serial <host:port>` to start already aimed at a TV) opens
an [egui](https://github.com/emilk/egui) window with the same reach as the CLI:

| Tab | What is there |
|---|---|
| Connect | type or scan for the TV, connect, list attached devices; each attached TV has a `use` button that retargets the whole window |
| Apps | scope filter, package table, tick rows, then disable / enable / uninstall / launch / force stop / info |
| Files | type a folder on the TV and a path on this machine, then list / send / copy from / make folder |
| Debloat | pick a scope and a UAD removal level, scan, tick, disable or re-enable |
| Settings | density, font scale, AdGuard DNS, OTA blocking, the on/off toggles, settings-screen shortcuts |
| Screen | screenshot (shown inline in the window), timed screen recording, scrcpy |
| Remote | every keycode as a button, plus text, URL, wake/sleep/reboot and the notification shade |
| Helpers | doctor, doctor --fix, device info, optimize, update check, `uad gui`, catalog installs (each row's `replace` button removes the other build of the same id first, e.g. official Shizuku → fork) |

Everything the GUI runs it runs through the same modules as the CLI, so a button
and its subcommand behave identically. The activity pane at the bottom keeps the
last 400 lines of what was executed; everything it prints is also appended to
~/.atvtools/log, which rotates to log.1 once it passes 1 MB.

## Original menu → this CLI

| Original (v4.0) | atvtools |
|---|---|
| IP connection screen (ARP scan, FIND/LOG/HELP) | `C` in the menu; `atvtools connect` scans and lists what answered, `connect <host>` connects, `pair <host:port> --code <6 digits>` for Android 11+ wireless debugging, `mdns`, `history` |
| A Remote Control (cmdmax.exe + RC_Emulator.bat) | `remote` (interactive keypad), `remote uudok` (scripted), `screen scrcpy`; `doctor` covers the console diagnostics |
| B Remove ads (AdGuard DNS / AdGuard TV) | `settings adblock-on`, `install adguard-tv` |
| 1 Transfer files / clipboard | `transfer push\|pull\|ls\|mkdir`, `device text` — no FTP app on the TV needed; the GUI's Files tab drives the same four |
| 2 Install / uninstall / enable / disable / debloat | `apps install\|uninstall\|disable\|enable\|backup`, `preset apply`; `--all-devices` runs an install, preset or toggle on every authorized TV |
| 3 Backup and restore | `apps backup`; restoring is `apps install` on the pulled APKs |
| 4 Custom settings (ad protection, NTP, storage grants, OTA, idle timeout) | `settings adblock-*`, `settings ntp`, `apps appop --all storage`, `settings ota`, `settings idle-timeout` |
| 5 Density / font scale | `settings set-density`, `settings set-font-scale` |
| 6 Screenshot / screenrecord | `screen shot`, `screen record` |
| 7 Installer helper (SmartTube, launchers, stores, IPTV) | `catalog`, `install <name>`, `iptv install\|add\|open` |
| 8 Optimizations (Perf_Optim.bat) | `optimize` |

## Not bundled as a binary, and why

- **cmdmax.exe** — its job was resizing console windows on Windows. The keypad
  is now `remote` in any terminal, the device list is `devices`/`history`,
  the console diagnostics are `doctor`, and mirroring is `screen scrcpy`.
- **Self-overwriting Dropbox updater** — the original downloaded a zip and
  unpacked it over its own folder, which is how "v5.0" turned out to be an older
  build. `update` only reports; `update --beacon <url>` still reads the old
  `version;url;changelog` file.
- **The UAD GUI** — not rewritten, but launched: `uad gui` downloads the
  upstream cross-platform build for this OS (0.5.1 ships `uad_gui-linux`,
  `uad_gui-macos`, `uad_gui-windows`) into `~/.atvtools/uad` and runs it with
  the same adb on PATH and `ANDROID_SERIAL` set to `-s`. Only the Windows
  build was ever downloaded by the original batch. The database queries are
  also ported natively (`uad list/info/apply`), so the window is optional.
- **PMX / Button Mapper configuration** — Android apps with their own UI.
  Install them with `store` and configure them on the TV.
- **Windows ADB driver installer** — Windows-only. `doctor` reports the adb
  version and where to get current platform-tools instead.
- **Google installer / Play Store / Google TV launcher APKs** — the original
  pulled them from XDA attachments, which rot. `google --dir <folder>` installs
  the copies you supply in dependency order: installer, services, store, launcher.
- **Countries list attachment** — a text file about geo-blocked IPs. `doctor`
  prints the egress country, which is the actionable part.
- **2000 lines of coloured `echo`** — replaced by the ANSI `menu` (EN/ES) plus
  subcommands, so the same steps can be scripted.

## Implementation notes

- Finding the TV: `adb mdns services` first, then the host neighbour table
  (`arp -an` / `ip neigh` / `arp -a`). Each candidate is TCP-checked on its adb
  port (400 ms) before `adb connect`, because adb's own connect can block for
  minutes on a host that answers nothing; at most 64 hosts are probed per scan.
- `adb` is invoked without a shell, arguments verbatim: the original's quoting
  bugs (`set cmdline=`, `FOR /F` parsing of `pm path`, `del "..."` gymnastics)
  cannot happen here.
- The device-side commands are the ones the batch script issues:
  `pm list packages -3/-s`, `pm uninstall --user 0`, `pm disable-user --user 0`,
  `pm enable`, `pm path` + `adb pull`, `wm density [reset]`,
  `settings put system font_scale`, `settings put global private_dns_mode`
  (and `private_dns_specifier dns.adguard.com`), `settings put global ntp_server`,
  `screencap`, `screenrecord --time-limit`.
- The debloat database is the upstream `uad_lists.json` fetched at runtime and
  cached in `~/.atvtools/uad_lists.json`; `uad refresh` re-downloads it. It is
  written for phones, so a few TV-only packages (for example
  `com.google.android.youtube.tv` and the leanback launchers) are absent from it
  and are covered by the built-in presets instead.
- Tests cover device-list parsing, `pm path` split parsing, density-line
  parsing, package-list de-duplication, catalog and preset lookups, permission
  scans, keycode/shorthand expansion, m3u name sanitising, APK manifest/resource
  parsing (with hand-built AXML, arsc and zip fixtures), diskstats size rows,
  the undo journal, the profile parser, macro scripts, the PNG reader behind the
  GUI screenshot preview, adb version detection, beacon parsing, version
  comparison against release tags, the Google install order and the UAD
  database levels/sorting. The GUI itself is not unit-tested: it is a thin shell
  over the same functions the tests cover, so it is exercised by running it.

## Feature parity with the original Windows tool

Status legend: **yes** = same job, one command; **partial** = the adb half is ported but the
step the original did with a bundled binary, an XDA attachment or the app's own UI is left to
the user; **no** = not ported.

| Original feature | Status | atvtools |
| --- | --- | --- |
| View/control the TV from the PC with scrcpy | yes | `screen scrcpy` |
| Remote control emulator (key events) | yes | `device key up/down/ok/home/playpause/...` (41 names, or a raw keycode) |
| Ad blocker (remove ads) | yes | `settings adblock-on` / `adblock-off` / `dns` |
| Install AdGuard for Android TV | yes | `install adguard-tv` |
| Send text from PC to TV | yes | `device text "hello world"` |
| Send files / fetch files from the TV | yes | `transfer push\|pull\|ls\|mkdir` (adb; no FTP app on the TV) |
| Sideload install / bulk install | yes | `apps install a.apk b.apk`, `apps restore <dir>` |
| Uninstall apps | yes | `apps uninstall <pkg...>` |
| Enable/disable apps | yes | `apps disable/enable <pkg...>` |
| Package name + version recognition | yes | `apps info <pkg>`, `apps list --paths --installer` |
| Universal Android Debloater | yes | `uad list/info/apply/refresh` runs the upstream database (about 2400 entries, Recommended/Advanced/Expert/Unsafe) natively, plus the TV-specific `preset` list; the GUI's Debloat tab drives the same scan. Upstream's own cross-platform window is on tap with `uad gui` (its `uad_gui-windows.exe` was all the original batch ever downloaded) |
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
| APK labels and versions (on-device aapt) | yes | `apk info <file>`, `apk list <dir>` — read on the PC, in-process, nothing written to /data/local/tmp |
| Dropbox version beacon | yes | `update --beacon <url>`; plain `update` checks GitHub releases |
| ES/EN localisation (two EXEs in the original) | yes | `menu --lang es`, or press L inside the menu |
| Country list helper | yes | `doctor` prints the egress country |
| Windows ADB driver install | partial | `doctor` reports the adb version and the platform-tools hint; it does not install drivers |
| Launch installed user apps | yes | `apps launch <pkg>` |
| Wake / sleep the TV | yes | `device wake`, `device sleep`, `device wake --lan` (Wake-on-LAN) |
| Check for TV updates | partial | `settings ota [--off\|--on]` blocks/allows OTA; firmware checks stay per-OEM |
| Open the notification shade | yes | `device notifications` |
| Device info (serial, build, Android version, resolution, battery) | yes | `info` |
| History of connected TVs | yes | `history` (also appended on `connect`) |
| (new) Device logs, crashes and bugreports on the PC | yes | `logs [--grep --follow]`, `crash`, `bugreport` |
| (new) Which packages are out of date | yes | `apps outdated`, `apps upgrade <name>` |
| (new) The same setup on several TVs | yes | `fleet status`, `fleet sync -f tv.atvtools.toml`, `preset export`, `apply --dry-run` |
| (new) Browser D-pad for the phone | yes | `remote --serve` |
| Suggest the TV IP | yes | `mdns` (adb mDNS discovery), `doctor` |
| Launcher shortcut icons for hidden apps | partial | `apps shortcuts` prints label + Play Store URL pairs (what the original's helper site consumed) |

**Counted:** 57 yes, 7 partial, 0 no of 64 listed items — roughly 88% of the listed
feature surface works with a single command; the partial ones are one manual step away.

The GUI adds no row: every tab is listed above as its CLI equivalent.
