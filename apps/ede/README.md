# Exponential Desktop (eDE internally)

The Exponential Desktop Environment (#NEW-10): a Wayland compositor plus a
shell drawn in genUI. This is its own Cargo workspace; it depends on the shared
libraries, never the other way round.

| Crate | Role |
|---|---|
| `exponential-session` | What the login screen starts: the session environment (`XDG_CURRENT_DESKTOP=Exponential`, Wayland hints for Firefox/Qt/Electron), `exponential-compositor` on the seat, then `exponential-session.target` (XDG autostart via systemd's generator, polkit agent, gnome-keyring, `exponential-device.service` = `exponential-session --device`: the Exponential app from its launcher entry, restarted when it crashes, so the machine stays an online device; nothing when the CLI daemon's own unit runs). Restarts a crashed compositor; three quick crashes in a row (also across logins) bring it back in safe mode. |
| `ede-protocol` | The control plane (#NEW-104): wire types, framing and a blocking `Client` for the compositor's socket, on Core's `Capability`/`Grant`/`Failure`/`Envelope`. Computer Use, eos-agent and the shell link it; it lives in `apps/desktop/crates/ede-protocol` (#EDE-51) and this workspace takes it by path. |
| `exponential-ui` | The shell's genUI on disk (#EDE-9): documents in `~/.config/exponential-desktop/ui` (factory copies in `/usr/share/exponential-desktop/ui`), validated before they apply, hot reloaded, the last good version kept, every edit a git commit, undo = revert. The shell and ede-tools link it; `exponential-ui` is its CLI. |
| `exponential-ctl` | The command line over `ede-protocol` (#EDE-7): `windows`, `outputs`, `capture --png` (the shots capture and the agent harness in the VM), `key`, `pointer`, `minimize`, `restore`. |
| `exponential-compositor` | smithay compositor: floating windows with magnetic edges and edge tiling, fullscreen, xdg-shell, wlr-layer-shell, Xwayland (X11 apps and games), linux-dmabuf, drm-syncobj explicit sync (udev), ext-session-lock, idle notify/inhibit, input methods (input-method-v2, text-input-v3, virtual keyboard), protected surfaces, damage-tracked rendering, `~/.config/exponential-desktop/compositor.toml`. Backends: `winit` (nested, development), `udev` (the real session: DRM/KMS, libinput, hotplug; feature `udev`) and `headless` (CI). |

## The shell (`apps/desktop/crates/ede-shell`)

`ede-shell` is a member of the IDE's Cargo workspace (#EDE-10, moved there by
#EDE-51: one lock and one target directory with the IDE and the SDK crates it
links): `exponential-shell` draws the shell from the genUI documents
`exponential-ui` serves (below) with gpui + eui-gpui at the SDK's pins (the
painter README's "gpui revisions", gated by `check-gpui-revs.ts`), Wayland
backend only (`cargo build -p ede-shell` in `apps/desktop`). The compositor
workspace never links gpui, and the shell is a separate process under the
supervisor, so a shell crash never takes the session down.

Since #EDE-50 the shell is `compositor + uicore`, four crates next to it
that the IDE will host too (`ide = shell + uicore`): `uicore-model`, the
pure shell reducer (`reduce(state, event)` + selectors: the desktop/team
switcher, the windows and their MRU, the rail's chats (#EDE-65),
Mint's grouped taskbar with pins, the start menu's categories and search,
the chat toggle, Alt-Tab), byte-locked by `packages/uicore/fixtures/shell.json`
and its TypeScript reference; `uicore-data`, what the documents bind (the
`Feed` of paths, the desktop entries, the clock, the compositor's window
list as reducer events, `/shell` from the model); `uicore-gpui`, the
`ShellHost` (`LayerShellHost`: one wlr-layer-shell surface per document,
popup documents and hosted Popover / Menu layers (VAPP-281) as
`xdg_popup`s of their parent; `WindowedHost`: normal windows for
`--windowed`) and the `Surfaces` that own the open windows; `uicore-docs`,
the factory documents compiled in. `exponential-shell` itself keeps what is
eDE's: ede-protocol, the notification server and tray host, the services
socket, the hotkeys, the `ede_*` host functions.

Every document with a layer
`surface` header is a layer-shell surface (`WindowKind::LayerShell`), opened
in the order the ground, the taskbar, the sidebar, then the rest (smithay
arranges layer surfaces in mapping order, so the taskbar takes the whole
bottom edge and the sidebar the left edge above it, #EDE-65); a popup document opens from its
anchor component in its parent as an `xdg_popup` of that surface, and so
does a Popover or Menu inside a document (hosted popups: the surface paints
none of it, the shell opens a popup window with the layer at (0, 0)). A
change on disk redraws the surface and the shell prints the document's
frame line, which the supervisor reads.
Data every document may bind: `/time` + `/date` + `/dateText` (local; the
calendar applet's weekday and date words), `/windows` (the
active workspace's windows over ede-protocol: id, title, app, focused,
minimized), `/shell` (the model: `chats` (the rail's), `taskbar.buttons` (Mint's groups:
app, pinned, windows, focused, minimized, running), `taskbar.previews`,
`workspaces.badges`, `startMenu` (open, query, category, categories, apps),
`chat.open`, `altTab`), `/apps` + `/query` (the launcher: the desktop
entries of the XDG data directories, filtered as the person types),
`/switcher` (Alt-Tab's order with the selection and live thumbnails),
`/previews` (a taskbar group's windows with thumbnails while they show),
`/osd` (a level). Host functions, each only from a document whose
`surface.functions` lists it (#EDE-8's Tier C rule, checked by `exponential-ui` and
again by the shell): `ede_launch {command}`, `ede_focus {window}`,
`ede_close {window}`, `ede_minimize {window}`, `ede_lock`, `ede_logout`,
`ede_power {action: suspend|reboot|poweroff}`, `ede_select {window}` (the
switcher), `ede_alt_tab_select {window}` (a hovered Alt-Tab tile selects it), and the model's: `ede_pin {app}` / `ede_unpin {app}` (kept in
`~/.config/exponential-desktop/shell.json`), `ede_taskbar {app}` (Mint's
click: launch, focus, minimize, or the previews of a group),
`ede_taskbar_scroll {app, direction}`, `ede_previews {app|null}`,
`ede_start_menu {open?, query?, category?}`, `ede_chat` (toggle; `{sessionId}`
opens a chat of the rail, #EDE-65), `ede_workspace {index}`,
`ede_history {action: undo|reset}` (the shell documents' history);
`/expo`, `/scale` and `/history` feed the overlays and the history.
Icons resolve by concept from `packages/icons` (the IDE's Lucide files and
fonts are embedded); app icons come from the installed icon themes as
`file:` pictures. Factory documents live in `apps/ede/ui/*.json` (the IDE
look, #EDE-42): `ground.json` (the background layer: the rounded panel the
focused window sits on, inset by the sidebar, the taskbar and the panel
margin), `sidebar.json` (left, the IDE's 56 px icon rail for good, #EDE-65:
only the chats, each the agent's mark with its dot; the IDE mounts the same
document and eDE prunes the IDE's header and corners), `taskbar.json`
(bottom, 48 px exclusive zone along the whole edge: the Exponential start
button, the chat button and the new-issue button bottom-left under the rail,
then Mint's grouped window list with pins, hover previews
and a context menu, the applets, the tray, the bell and the clock),
`start-menu.json` (the start button's popup: Mint's three columns and the
search box; its own entries are Files (Nemo, else the default file manager),
Version history and the Exponential app when installed, #EDE-53), `chat.json`
(the Assistant's composer, on demand), `history.json` (Version history: the
shell documents' commits with Undo and Reset to factory, on demand),
`switcher.json`, `expo.json` and `scale.json` (`onDemand` overlays the shell
opens on a hotkey and closes again: Alt+Tab, Ctrl+Alt+Up with one tile per
workspace, Ctrl+Alt+Down with the active workspace's windows, each with live
thumbnails, #EDE-49), `osd.json`, `notifications.json` (the toasts, top-right, on
demand), `notification-center.json` (the bell's popup) and `tray-menu.json`
(a tray item's menu as the tray's popup). A click outside an open popup, or
Escape in it, closes it. A layer document's root is opaque (`$color.background`
for the ground, the sidebar and the taskbar, `$color.popover` for Settings,
the OSD and the switcher): `$color.card` is a white
glaze at 6% in the dark theme, right inside a window, see-through for a
surface floating over other windows (#EDE-60; `crates/ede-ui/tests/factory.rs`
checks every layer document).

### Notifications and the tray (#EDE-12)

The shell is the session's `org.freedesktop.Notifications` server (toasts
with the app's icon and actions, the app's timeout or 6 s, critical ones
stay; the history behind the panel's bell with its count) and its
`org.kde.StatusNotifierWatcher` + host: items register, the panel shows
their icon (a theme name, the item's own theme path, or its pixmap), a
click activates the item or opens its `com.canonical.dbusmenu` as a popup
and a row press sends the item its `clicked` event. Nested under another
desktop the well-known names stay with that desktop and the objects answer
on the shell's unique bus name (`scripts/tray-demo.py --watcher :1.N`
registers a sample item; `gdbus call --dest :1.N …Notify` a sample toast).
Host functions: `ede_notify_close {id}`, `ede_notify_action {id, key}`,
`ede_notify_clear`, `ede_tray {service}`, `ede_tray_menu {service, id}`.

### Hotkeys (#EDE-12)

The compositor keeps these keys and tells the shell over ede-protocol
(`hotkey` events; the shell's layer surfaces never hold the keyboard while
an app has it): **Alt+Tab** opens the switcher and cycles the current
workspace's windows (Shift or the arrow keys reverse; Mint's look and
100 ms delay, so a quick Alt+Tab flips windows with no popup), **Alt+`**
(the key above Tab) cycles the focused app's windows, releasing Alt
focuses the selected window (restoring a minimized one), Escape cancels, a
click on a tile picks it; **Super** alone toggles the menu (its search box
takes the keyboard); **Ctrl+Alt+Up** toggles Expo and **Ctrl+Alt+Down**
Scale (`expo.toggle`, `scale.toggle`, #EDE-49: the shell's overlays, no
fallback without one); **Print** saves `~/Pictures/Screenshots/Screenshot
<date>.png`; **Ctrl+Alt+T** opens the terminal; the **volume, microphone
and brightness keys** change the level (WirePlumber's `wpctl`, sysfs or
`brightnessctl`) and show the OSD; play/next/previous go to `playerctl`.
With no shell listening (dead, not started) the compositor falls back:
Alt+Tab cycles focus itself, most recent first, and the media keys reach
the app. `exponential-ctl key <code> --bindings` sends a key as the person's (the
root grant only), which is how the goldens and an agent in the VM drive
them.

```bash
apps/ede/scripts/ede-dev.sh                   # nested in the desktop around it: compositor + supervisor + shell
apps/ede/scripts/ede-dev.sh --headless 1280x720
WAYLAND_DISPLAY=wayland-1 target/debug/exponential-ctl capture --png /tmp/panel.png --expect-drawn
WAYLAND_DISPLAY=wayland-1 target/debug/exponential-ctl pointer 40 20 --click   # the menu
```

The M0 demo (#EDE-1), in the `ubuntu-wayland` VM: edit the scratch
directory's `taskbar.json` and the taskbar redraws within the watcher's
debounce; break it and `exponential-ui check` says why while the last good version
stays on screen; `git -C <dir> log` shows `Edit taskbar.json` commits after
the factory ones. `exponential-shell --windowed` opens normal windows instead
(macOS, any desktop); `--popup` opens every popup document right away and
`--exit-after <ms>` quits, for the headless check.

## Run it nested (Cinnamon/X11)

```bash
cd apps/ede
cargo run -- -- gnome-calculator     # anything after `--` starts inside eDE
cargo run -- -- xterm                # X11 apps run through Xwayland
WAYLAND_DISPLAY=wayland-1 xed        # or point a client at the socket it logs
```

Driving it with `xdotool` on the live desktop sends keys to whatever window
is focused there, not necessarily eDE: script input against a private
`Xvfb :77` (Mesa EGL: `__EGL_VENDOR_LIBRARY_FILENAMES=/usr/share/glvnd/egl_vendor.d/50_mesa.json`).

## Tier A: frames, the recovery screen, the restore hotkey (#EDE-16)

What the compositor draws itself, without gpui, from colour tokens only,
never agent-editable (`crates/ede-compositor/src/{text,frames,recovery}.rs`):

- **Window frames**: a client that leaves decorations to the compositor
  (xdg-decoration, or an X11 window without its own per its Motif hints)
  gets a 36 px title bar: the title, maximize and close, a draggable bar
  (double-click maximizes), a 6 px resize band around the window. The frame
  stays when maximized or tiled, goes when fullscreen. A client that
  insists on its own decorations (GTK 4, libadwaita) keeps them.
- **Recovery screen**: while the shell is not subscribed to ede-protocol
  (never within 20 s of start, or gone for 8 s) every output shows what to
  do: Super+Shift+R, Ctrl+Alt+T (the compositor opens the terminal itself
  then), Super+Shift+E.
- **Super+Shift+R**: `exponential-ui reset` (every factory document back, as
  commits; the person's edits stay in the history) and `systemctl --user
  restart exponential-shell.service`; `EDE_RESTORE_COMMAND` replaces the command.
  It needs nothing from the shell.
- Fallback focus cycling (Alt+Tab without a shell) is in [Hotkeys](#hotkeys-ede-12).
- The consent shield and the takeover banner wait for the agent spaces (M2).

## Settings (#EDE-14, #EDE-47)

`settings.json`, a shell document like the rest (editable, in the
history), not an eApps package. #EDE-65: Settings open into the sidebar
like the IDE's: ONE on-demand top layer over the rail and the ground,
standing on the taskbar (`exclusiveZone` -1, `margin.bottom` 48; the
menu's Settings rows open it with `ede_open_document`, its close button
`ede_close_document`): the settings nav takes the rail's place, 272 px
(`settingsWidth`, the IDE's settings column), and the page sits on the
ground's rounded panel, no tile overview. The nav: **This device** (Display: every output's
modes and scale, `ede_output` applies live and writes `[[outputs]]`; Sound:
volumes, mute, the output and input devices, the playback streams; Network:
Wi-Fi, wired, the networks in reach, VPN; Bluetooth; Power: battery,
profile, suspend/restart/shut down; Themes: the bridge's switch and what it
resolved, #EDE-19; Keyboard: the xkb layouts of `evdev.lst` to pick from,
repeat; Mouse & Touchpad; Date & Time: NTP, the time zone, the clock, the
inline calendar on `/date`; Users; Notifications: this desktop's, then the
Exponential app's email/push rows; Workspaces; Windows: edge tiling, the
snap distance, the panel margin; Startup apps: the XDG autostart entries
with a switch; Default apps: the xdg-mime roles with the installed handlers;
then the folded **Coding** group, the IDE's This device group: Tools, Coding
CLIs, Worktrees, Sessions), **Personal** (Account, Devices, Security, What's
new, About with the system check, updates and printers) and **Team**, headed
by the shell's desktop/team switcher (its entries as a Popover; picking one
here changes the window only, never the shell's desktop): a Local desktop
shows General (the name, Share as a team…), Agents, Apps; a team entry the
IDE's Team, Boards, Agents and Apps groups of `settings-nav.json` (eDE has
none yet, #EDE-54). Pages bind `/services/<domain>` (#EDE-13), `/outputs`,
`/config`, `/user`, `/shell`, `/date` and `/pages` (the shell's own:
`crates/ede-shell/src/settings_pages.rs` reads the layouts, the time zone,
the autostart entries, the mime defaults, `[windows]`); a control is two-way
bound and calls `ede_service {domain, action, args}`, `ede_output`,
`ede_config {theme}` or `ede_settings {page, key, value}` (the one host
function for the shell's pages: `[keyboard]` and `[windows]` in
`compositor.toml` + a reload, `timedatectl`, the user's autostart override
with `Hidden`, `xdg-mime default`). A setting the machine cannot apply yet
(the pointer options, the workspace count and wrap-around, the 24-hour
clock) is a read-only value row, never a dead control; what needs the
Exponential app (the Coding pages, Devices, Security, the notification
preferences, a team's pages) is an honest empty state until it runs on this
desktop. `EDE_SHELL_SETTINGS_PAGE=<page>[,switcher]` under `EDE_SHELL_DEMO`
opens a golden on that page (the goldens `ede-settings`,
`ede-settings-team`).

## The portal: `xdg-desktop-portal-exponential` (#EDE-15)

`crates/ede-portal`, the xdg-desktop-portal backend the frontend routes
ScreenCast, Screenshot, RemoteDesktop, GlobalShortcuts and Settings to
(`exponential-portals.conf`; the GTK backend keeps the file chooser and the rest),
D-Bus activated in the session (`xdg-desktop-portal-exponential.service`). Every
call is an ede-protocol call under a grant minted for the portal
(`capture`, `input:person`, `ui`), never the root token. A ScreenCast is
a PipeWire node fed by captures polled at the stream's rate with `since`:
when nothing was presented the compositor answers `unchanged` and draws
nothing; the GPU (dmabuf) path for udev is still to come. What to share
is the sealed chooser `exponential-sealed share` (`[screencast] chooser` in
`~/.config/exponential-desktop/portal.toml` replaces it): screens and windows to pick,
the remote-control switch, Share or Cancel; remote control is never
granted silently (the switch, or `[remote_desktop] allow`). Global
shortcuts follow the XDG spec (`CTRL+SHIFT+a`), never take a plain key or
a compositor binding, never fire on the lock or a protected surface.
Settings serves `org.freedesktop.appearance` (dark, the accent). Tests
run a private dbus-daemon, a private PipeWire and a headless compositor.

## Services: `exponential-services` (#EDE-13)

One service per domain over the daemons the distribution runs, each with
two faces on one registry (`crates/ede-services`): `network`
(NetworkManager: state, devices, Wi-Fi access points, `wired` = the
ethernet devices `[{path, interface, state, connection}]`, `vpn` = the
saved VPN and WireGuard connections `[{id, name, type, active}]`,
`wifiEnabled`, `wiredConnected`; connect, disconnect, `wifi_enable {on}`,
`wired_enable {on}`, `vpn_connect {id}`, `vpn_disconnect {id}`), `sound`
(PipeWire through `wpctl`: default sink and source, volumes, mutes,
`sinks` and `sources` `[{id, name, volume, muted, default}]`, `streams` =
the playback streams of the mixer `[{id, app, name, volume, muted}]`
(#EDE-43); set, mute, `set_default {id}`, `set_default_source {id}`,
`set_stream_volume {id, percent}`, `mute_stream {id, on}`), `media`
(MPRIS players on the session bus: `players: [{name, identity, status
(playing|paused|stopped), title, artist, album, artUrl, position, length
(seconds), canGoNext, canGoPrevious, canSeek}]`, `active` = the first
playing one else the first, none = an empty list; `play_pause`, `next`,
`previous`, `stop` and `seek {position}` take an optional `name`, unset =
the active player), `bluetooth` (BlueZ: adapters, devices, `powered`,
`discovering`; power, connect, disconnect, discover), `power` (UPower
`battery: {present, percent, state (charging|discharging|full|empty|
unknown), timeText}` and `batteryText` (`85% · 2:10 left`),
power-profiles-daemon profile, logind: suspend, hibernate, reboot, power
off), `disks` (udisks2: drives, filesystems; mount, unmount), `updates`
(apt: upgradable packages; refresh, upgrade through `pkexec`), `firmware`
(fwupd), `about` (host, OS, kernel, CPU, memory). A missing daemon gives
`{available: false, reason}`, so a page can say so.

- **Data source**: `$XDG_RUNTIME_DIR/exponential-desktop/services.sock`, ede-protocol
  framing, `list`, `get {domain}`, `call {domain, action, args}` (every
  capability: the shell's own buttons), `subscribe` (`services.changed`
  per domain whose snapshot moved, polled every 2 s). The shell feeds
  every document `/services/<domain>`; the settings pages (#EDE-14) bind
  it. `exponential-services get sound`, `exponential-services call sound set_volume
  '{"percent": 40}'` from a terminal.
- **MCP face**: eMCP's loopback server (`exponential-services tools` lists them:
  `<domain>_get` and `<domain>_<action>`), a bearer per agent session from
  `exponential-services grant <session> [label] [system,ui]` (over the socket;
  ede-tools wires it in M2), each action gated by the eCore capability its
  description names (`ui` for sound and media, `system` for the rest;
  `crates/ede-services/tests/tools.rs` pins the list), a refusal
  answered as `isError` content, and an `mcp_calls`-shaped row per call in
  `$XDG_STATE_HOME/exponential-desktop/mcp-audit.jsonl` (audit.json's columns, the
  canonical `args_hash`).
- `exponential-services.service` runs it in the session.

## Tier B: `exponential-sealed` (#EDE-17)

The surfaces a document on disk must never be: compiled into
`exponential-sealed` (`apps/ede/sealed/*.json`, a second binary of the shell
workspace, drawn with the same gpui + eui-gpui and tokens), each a
full-screen overlay with the keyboard to itself. The compositor marks every
layer surface of that executable as protected (black in captures, never
driven, no input method), and trusts the lock client by the pid it spawned:

- **Lock** (`exponential-sealed lock`): with no `[lock] command` the compositor
  starts it on Super+L, idle and suspend; while locked only that process's
  surfaces show (black until one arrives), only they get input, bindings
  are off except VT switching. The password goes to PAM inside the binary
  (`libpam.so.0` opened at run time; service `ede` when `/etc/pam.d/exponential-desktop`
  exists, else `login`), and the session opens only through the `unlock`
  op with the grant the compositor minted for that process (`EDE_TOKEN`).
  A lock client that dies keeps the session locked and is started again.
  `swaylock` still works through `[lock] command` (ext-session-lock).
- **polkit** (`exponential-sealed polkit`, `exponential-polkit-agent.service`): the
  session's authentication agent; polkitd's request opens the prompt, the
  password goes to `polkit-agent-helper-1`.
- **Consent** (`exponential-sealed consent --title … --body … [--allow …] [--deny
  …]`): a question to the person for an agent's privileged action; prints
  `allow` or `deny`, exits 0 or 1 (eMCP/ede-tools call it in M2).
- Not done: a keyring prompt of our own (gnome-keyring unlocks at login
  through PAM and keeps its own prompter), the accent-only theming.

## Outputs: mode, scale, position (#EDE-18)

`~/.config/exponential-desktop/compositor.toml` names each output by connector:

```toml
[[outputs]]
name = "DP-1"
mode = "5120x2880@60"   # WxH or WxH@R; unset = the preferred mode
scale = 2.0             # 0.5…4, fractional too (1.25, 1.5): wp-fractional-scale + wp-viewporter
position = [0, 0]       # logical px; unset = next to the previous output
[[outputs]]
name = "DP-2"
scale = 2.0
position = [2560, 0]
```

An output not listed takes its preferred mode, scale 1 and the next place
to the right. `ede-protocol`'s `list_outputs` lists every mode, the current
and preferred one, make, model and physical size; `set_output {name, mode?,
scale?, position?}` (`system`) applies a change live, the Displays page
(#EDE-14) persists it to the file and `reload_config` re-applies the file.
A modeset on udev goes through the DRM output (`use_mode`); nested and
headless outputs resize. A connector that changes while connected (udev's
hotplug: a VM window resized, a monitor re-reading its EDID) follows its
new preferred mode unless `[[outputs]]` pins a mode it still offers; the
scan runs 150 ms after the last change (VBoxDRMClient pushes a layout
every 250 ms while the window is dragged), then the outputs without a
configured position close ranks, layers re-arrange and tiled windows
follow. In VirtualBox this is the guest additions' resize: `VBoxDRMClient`
(the `virtualbox-guest-utils` system service, no session service needed)
pushes the window size to `vmwgfx`, which lists it as the preferred mode. Every surface hears its output's preferred scale
(fractional through wp-fractional-scale-v1; integer clients keep
`wl_surface.set_buffer_scale`). Direct scanout: a fullscreen window alone
on an output is the frame's only element, and smithay's DRM compositor
puts it straight on the primary plane when its buffer fits (the default
frame flags allow it); the udev backend sends such windows a dmabuf
feedback with a scanout tranche of the plane's formats so games and video
allocate scanout-able buffers. Hybrid GPUs: one render GPU (the primary),
outputs on the other card render through it (smithay's multi-GPU
renderer); a per-output render device is not done.

## The parity gate (#EDE-45)

`bun apps/ede/scripts/parity.ts --out /tmp/ede-parity.png` draws the contact
sheet: every region of the IDE's shell (its committed desktop shots) next to
Exponential Desktop's (the `ede-*` goldens), one row per region with the
status `packages/uicore/parity.md` gives it, and prints the inventory's
share of `same` or `close` rows (the gate wants ≥ 90 %). Danny signs the
sheet off by eye; nothing in the IDE is replaced before that.

## Doctor

```bash
cargo run -- doctor
```

Checks what the real session needs, from inside any desktop (no seat is
taken; `--json` is what Settings shows, #EDE-35). The rows:

- **GPU**: the render nodes. **DRM**: the KMS driver of every
  `/dev/dri/card*` and its connected displays (the virtual ones say what to
  expect: `vmwgfx` is VMware's and VirtualBox's VMSVGA, software rendering
  unless the VM has 3D); with more than one card, which one is the boot GPU
  and which one the session renders on: the card with the displays
  (`doctor::primary_card`, the same rule the udev backend uses), so a
  desktop whose monitors hang off the discrete card while the firmware
  booted on the iGPU never renders on the iGPU and copies every frame.
- **KMS**: what the kernel offers on that card, read without DRM master so
  it works next to a running desktop: atomic modesetting (legacy = no
  cursor plane, no direct scanout; on NVIDIA it means `modeset` is off),
  the cursor planes and their size, the primary/overlay planes, framebuffer
  modifiers (tiled buffers straight on the plane), syncobj + timeline
  (explicit sync) and `IN_FENCE_FD` (the kernel waits for the renderer;
  without it the compositor waits itself, as on vmwgfx).
- **seat**: logind's `seat0` with a graphical device, or seatd.
- **render**: what EGL and GLES render with on the render node, the way
  the udev backend opens it. `llvmpipe` is fine in a VM; on a real GPU's
  driver it is a failure, and the session refuses to run that way (below).
  A udev build only; a winit-only build says so.
- **Vulkan**: the GPU the shell takes (`exponential-shell` is gpui on
  Vulkan, which takes the loader's first device): `vulkaninfo --summary`'s
  GPU0 against the compositor's card, lavapipe first = software (a failure
  on a real GPU; `MESA_VK_DEVICE_SELECT=<vendor:device>!` puts the right
  one first). Without vulkan-tools, the ICD manifests found.
- **keyboard**: the layout the session starts with (`/etc/default/keyboard`,
  else xkb's `us`).
- **Xwayland**: the binary the compositor will run and its version (24.1+
  has explicit sync; older versions flicker with X11 games on NVIDIA; the
  .deb bundles 24.1).
- **NVIDIA**, **NVIDIA GBM**, **NVIDIA sleep** when `nvidia_drm` is
  loaded (#EDE-59, below).

### NVIDIA (#EDE-59)

The proprietary driver (and its open kernel module) needs, in order:

1. **Driver 525 or newer** (GBM with modifiers, EGL render nodes), **555+
   for explicit sync** (`linux-drm-syncobj-v1`: no flicker in Vulkan apps
   and X11 games), 560+ turns the next two on by default. The row names
   the kernel module: `open kernel module` (`nvidia-open`, "Dual MIT/GPL",
   NVIDIA's recommendation for Turing and newer from 560) or `proprietary
   kernel module`; both work the same for eDE, read from
   `/proc/driver/nvidia/version`.
2. **`nvidia-drm.modeset=1`**: without it nvidia-drm has no atomic KMS
   and no display can be driven (the compositor refuses the card with that
   reason). The doctor reads the module parameter (root-only, 0400, on
   most distros), else the kernel command line, else `options nvidia_drm
   modeset=1` in `/etc/modprobe.d` and `/lib/modprobe.d` (where the
   driver's package and nvidia-prime write it), else the driver's default,
   and says which one it used. **`fbdev=1`** (recommended) gives a console
   framebuffer: a flicker-free VT switch and a readable console after a
   crash.
3. **The GBM backend**: Mesa's libgbm loads `nvidia-drm_gbm.so` for the
   `nvidia-drm` KMS driver, and libEGL_nvidia needs its GBM platform
   (`/usr/share/egl/egl_external_platform.d/15_nvidia_gbm.json`). Both
   ship with the driver's GL libraries (Ubuntu, Mint: `libnvidia-gl-NNN`;
   Fedora/RPM Fusion: `xorg-x11-drv-nvidia-libs`; Arch: `egl-gbm`).
   Without them `EGLDisplay::new(gbm)` fails and the render row fails.
4. **Video memory across suspend**: `NVreg_PreserveVideoMemoryAllocations=1`
   (`/proc/driver/nvidia/params`) plus `nvidia-suspend.service` and
   `nvidia-resume.service` enabled; otherwise the GPU forgets its memory
   while asleep and the desktop and every GL/Vulkan app come back garbled.
   The compositor drops its own caches on every resume anyway (fresh
   swapchain buffers, cursor images uploaded again).

What the udev backend does on NVIDIA: atomic KMS with the cursor planes
and no overlay planes (they break on the driver); GBM buffers with the
modifiers the primary plane lists, which is also the scanout tranche a
fullscreen window hears through dmabuf feedback (direct scanout); explicit
sync whenever the card has syncobj eventfds; one log line per card with
the KMS facts (`nvidia-drm: atomic, 4 cursor planes 256x256, … modifiers,
syncobj timeline, IN_FENCE_FD`) and one per output (format, plane formats,
planes in use) so the first run on real hardware tells what it got.
**Software rendering is never silent**: a real GPU's driver (nvidia,
amdgpu, i915, xe, nouveau, …) whose EGL device is Mesa's llvmpipe stops the
session with the reason on the recovery screen, where the safe-mode login
entry (stock settings) still runs it in software to fix the driver from.
Unproven on hardware as of eDE 0.2.5 (the VM has no GPU; the checks are
tested against `crates/ede-compositor/tests/fixtures/doctor/mint-rtx4090`,
the files a Mint 22.3 box with an RTX 4090 on driver 595 showed): the GLES
renderer on the NVIDIA EGL device, the cursor plane, direct scanout, the
shell's Vulkan swapchain on a layer-shell surface, suspend.

Checking a machine without touching its desktop, in this order:

```bash
sudo apt update && sudo apt install exponential-desktop
exponential-compositor doctor            # from the running desktop: read-only, no seat
exponential-compositor                   # nested in a window of the running desktop (winit)
apps/ede/scripts/ede-dev.sh              # nested with the supervisor and the shell
```

Then the real seat: log out, pick **Exponential** on the login screen
(**Exponential (safe mode)** is the fallback; a session that cannot drive
the display shows why and returns to the login screen), and read
`journalctl -b | grep -E 'ede_compositor|exponential'` for the KMS and
output lines above. Two monitors on the NVIDIA card take their native
modes; with the X11 setup's virtual 5K the equivalent is
`[[outputs]]` with `mode = "3840x2160@240"` / `"2560x1440@144"` and a
`scale` ([Outputs](#outputs-mode-scale-position-ede-18)).

The compositor opens one window on the running X11 (or Wayland) session and
serves `$XDG_RUNTIME_DIR/wayland-N`. Close the window to quit.
`--headless 1920x1080` runs without a window (pixman, no GPU).

Bindings: Alt+drag moves a window, dropping it on a side edge tiles it to a
half (corners = quarter, top = maximize), Super+Left/Right tile, Super+Up
maximizes, Super+Down restores, Alt+F4 closes, Super+L locks, Super+Shift+E quits
(logs out of the session), Ctrl+Alt+F1…F12 switch VTs (udev). Workspaces
(#EDE-18, shared by every output as on Cinnamon, four listed, up to nine):
Super+1…9 switch, Super+Shift+1…9 take the focused window along,
Ctrl+Alt+Left/Right step (with Shift: the window too). A window on another
workspace is parked off screen; the taskbar still lists it and focusing it
switches back. Over ede-protocol: `list_workspaces`, `switch_workspace`,
`move_to_workspace`, the `workspace.switched` event.

## Run it as the session (a TTY)

```bash
cargo build --release --features udev
# switch to a free TTY (Ctrl+Alt+F3), log in, then:
target/release/exponential-compositor -- gnome-calculator
```

With neither `DISPLAY` nor `WAYLAND_DISPLAY` set it takes the seat through
libseat (logind or seatd): every connected monitor becomes an output, left
to right; plugging one in or out re-lays them and moves stranded windows to
the first. `EDE_DRM_DEVICE=/dev/dri/cardN` picks the GPU. Started inside a
running X11/Wayland session it nests instead; never pass `--udev` there,
the other display server owns the GPU. The login screen entry is
`exponential-session` (below).

## Install as a login session

From the package repositories (#EDE-34; Ubuntu 24.04 + 26.04, Mint 22,
Debian 13, Fedora, Arch; the docs' install page has the per-distro lines):

```bash
curl -fsSL https://exponential.at/desktop/setup.sh | sudo sh
```

The repositories are static and signed, built by `build-ede.yml` from an
`ede-v*` tag: `packaging/stage.sh` builds once on Ubuntu 24.04,
`pack-{deb,rpm,arch}.sh` pack that tree, `repo/publish.sh` writes the
apt/dnf/pacman metadata and signs it (key `repo/exponential.asc`, its secret
half the `PKG_SIGNING_KEY` secret), every distro of the matrix installs it in
its container (`smoke.sh`), then the tree is rsynced to the package host (a
Caddy container on the Hetzner host, Coolify service `exponential-pkg`,
serving `/srv/exponential-pkg/www`). The host name lives in
`packaging/repo.env` only; `exponential-repo` (installed alongside) owns
each machine's source entry and key, so a new host or key arrives as an
update. A local .deb:

```bash
apps/ede/packaging/build-deb.sh          # → apps/ede/target-deb/exponential-desktop_<version>-1~noble1_amd64.deb
sudo apt install ./apps/ede/target-deb/exponential-desktop_*_amd64.deb
```

The .deb bundles Xwayland 24.1 at `/usr/lib/exponential-desktop/bin/Xwayland`, because
Mint 22 / Ubuntu noble ships 23.2.6, which has no explicit sync.
`packaging/xwayland/build.sh` builds it: a pinned, sha256-checked tarball,
built with meson against noble's own libraries. It runs in an
`ubuntu:24.04` container when the host has no meson, and the result is
reused until the script changes. The compositor puts `/usr/lib/exponential-desktop/bin`
first on Xwayland's `PATH`. `--no-xwayland` skips the bundled build (quick
local builds) and depends on the distro's `xwayland`. CI: `build-ede.yml`
(`ede-v*` tags, by hand, PRs that touch `packaging/`).

Check on the real machine after installing: `exponential-compositor doctor` shows
`ok Xwayland /usr/lib/exponential-desktop/bin/Xwayland 24.1.x, explicit sync`. In the
session, an X11 game (Steam/Proton) on NVIDIA no longer flickers, and
`pgrep -a Xwayland` shows `/usr/lib/exponential-desktop/bin/Xwayland`.

The package carries the compositor, `exponential-session`, `exponential-ui` and `exponential-shell`
in `/usr/bin`, the factory documents in `/usr/share/exponential-desktop/ui`, and the
session's user units (#EDE-2): once the compositor is up, `exponential-session
--ready` hands `WAYLAND_DISPLAY` to systemd and starts `exponential-session.target`,
which wants `exponential-shell.service` = `exponential-ui supervise -- exponential-shell`, so a
login gets the panel under the supervisor (a crashed shell restarts, a bad
edit rolls back; `systemctl --user status exponential-shell` and `journalctl --user
-u exponential-shell` show it). `--dev` packages the dev profile's binaries for a
quick check in a VM.

Log out; the login screen's session menu (LightDM on Mint, GDM's gear icon
on Ubuntu, both read `/usr/share/wayland-sessions`) now has **Exponential**
and **Exponential (safe mode)** next to Cinnamon or Ubuntu. Safe mode
starts with the stock settings (`--safe-mode`: the config file is not read;
`EDE_SAFE_MODE=1` reaches the shell's unit, and `exponential-ui` then supervises a
scratch copy of the factory documents, leaving `~/.config/exponential-desktop/ui`
untouched). Super+Shift+E logs out; Ctrl+Alt+F1…F12 switch VTs. Logs:
`journalctl --user -b` (the units) and `journalctl -b _COMM=exponential-session
_COMM=exponential-compositor` (the session itself; LightDM also writes
`~/.xsession-errors`). The lock screen authenticates through
`/etc/pam.d/exponential-desktop` (a conffile). Remove with `sudo apt remove ede`; Cinnamon
and GNOME are untouched.

If eDE does not come up, the screen says so (#EDE-40): a compositor that
cannot drive the display stops itself (exit code 3, the reason in
`$XDG_RUNTIME_DIR/exponential-desktop/failure`, and one that never gets a frame out after
20 attempts counts), and `exponential-session` paints the reason and what happens
next onto every display with `exponential-compositor recovery` (a DRM dumb buffer
and the Tier A canvas, no GPU involved), then falls back: a crash restarts
the compositor, three quick crashes in a row (also across logins) land in
safe mode, a fatal start gets safe mode once and then the login screen,
where the previous session is still in the menu. Any key skips the wait.
Ctrl+Alt+F3 gives a text login; `journalctl -b _COMM=exponential-compositor
_COMM=exponential-session` has the details, `exponential-compositor doctor` what EGL
renders with. Ending the session from the login manager (SIGTERM) stops
the compositor and both session targets, so no unit keeps restarting the
shell against a gone display. In a VirtualBox VM keep the VMSVGA graphics
controller (the `vmwgfx` driver, its "unsupported hypervisor" kernel line
is normal); 3D acceleration is not needed, eDE renders in software there
like GNOME does: Mesa's EGL device is then a software device that names
no render node, and the compositor renders through the card's own
(`renderD128`). Such a plane has no `IN_FENCE_FD`, so a frame waits for
llvmpipe before its flip (smithay's `needs_sync`); without the wait the
host showed half-drawn buffers as flicker. `exponential-compositor recovery --reason … --png out.png --size
1280x800` draws the message into a file for a look.

## ede-protocol

`$XDG_RUNTIME_DIR/exponential-desktop/<wayland-N>.sock` (directory 0700): frames of a
little-endian `u32` length plus JSON (`{"id": 1, "op": "list_windows"}`),
the pixels of a capture as the frame after its reply. `hello` with a token
first; each op needs the Core capability it names:

| Op | Capability |
|---|---|
| `list_windows`, `list_outputs`, `pointer_position`, `subscribe` (events: `window.opened/closed/focused/changed/minimized/restored`, `session.locked/unlocked`, `config.reloaded`) | any grant |
| `focus_window` (a minimized window comes back first, a parked one brings its workspace along), `close_window`, `move_window`, `minimize_window`, `restore_window` (#EDE-48: into the taskbar and back; a minimized window keeps its geometry and workspace, `WindowInfo.minimized`; `exponential-ctl minimize <id>` / `restore <id>`) | `ui` |
| `capture` (an output: protected windows black, locked = black; a window: protected ones refused) | `capture` |
| `input` (key by evdev code, `keysym` = the key that types it on the active layout with its Shift/AltGr held around it, pointer motion, button, wheel; refused on protected surfaces and the lock) | `input:person` |
| `hold_input`, `release_input` (#EDE-23: the person's keys, pointer and wheel reach no window while the connection lives, the agent's `input` still does; their Esc ends every hold and reaches the holders as `input.escape`; the lock screen always takes their input) | `input:person` |
| `reload_config`, `lock`, `mint_grant`, `revoke_grant` | `system` |

The compositor writes the root token (every capability) to
`$XDG_RUNTIME_DIR/exponential-desktop/<wayland-N>.token` (0600) for the shell; `mint_grant`
hands narrower, expiring grants to agents (never more than the minting grant
holds, never outliving it). The root token is the shell's and
`exponential-ctl`'s; every other client (the portal, ePilot, an agent)
connects with `Client::connect_as(session, capabilities, expires)`, which
mints its own grant and drops the root connection (#EDE-22: grants by
default); a process handed a grant in `EDE_TOKEN` keeps that one. Any process of the same user can read the token
file, so the boundary is between the shell and the agents it mints for (an
agent sandbox does not see `$XDG_RUNTIME_DIR/exponential-desktop`). Agent spaces and window
lending come with their own issues; captures are drawn in software for now.

```rust
let mut ede = ede_protocol::Client::connect_env()?;   // the shell: $WAYLAND_DISPLAY + the token file (or $EDE_TOKEN)
for w in ede.windows()? { println!("{} {:?}", w.id, w.title); }
// Anyone else: a grant of its own, minted from the root token and no wider than it needs.
let mut agent = ede_protocol::Client::connect_as(SessionId::new("my-agent")?, vec![Capability::Capture], None)?;
```

### ePilot on eDE (#EDE-23)

ePilot (`apps/desktop/crates/epilot`) drives an eDE session through this
socket instead of XTest: `XDG_CURRENT_DESKTOP=Exponential` and the socket
present = the eDE backend (`epilot::backend_name()` says `eDE`). On first
use it mints its own grant (`capture`, `input:person`) from the root token
and keeps it for the process; a tool call is `input` (keysyms, so the
compositor's layout decides the key) / `capture` / `list_windows`; its lock
is `hold_input` on a connection of its own, so the IDE's banner and Esc
work as on X11. Headless test: `EDE_COMPOSITOR=<target>/debug/exponential-compositor
cargo test -p epilot headless_ede` (in `apps/desktop`).

## Apps (#EDE-20, #VAPP-273)

`APPS.md`: Mint's default apps against what eDE keeps, replaces with the
shell and settings, or rewrites as Apps packages on the Engine; Calculator,
System Monitor and Software are listed there with what each needs first.

### Apps on the Engine (#VAPP-273)

The shell hosts `eengine` (docs/spec/apps.md §16): every
`/usr/share/exponential-desktop/apps/*.app.json` (the factory apps the
package installs from `apps/ede/apps`; `EDE_FACTORY_APPS_DIR` overrides)
and every `~/.config/exponential-desktop/apps/*.app.json` (next to the
`ui/` documents; `EDE_APPS_DIR` overrides; a file of the same name replaces
the factory one) is one app, validated by genUI's own
validator at boot (a refused one is logged and skipped) and run as an
exported app would run on the desktop runner: the same `app`, `consent`,
`permissions` and `toast` surfaces (`packages/eengine/fixtures/runner.json`),
the identity `local` (this device, no account), the host kind `ede`.
Records of every app live in one `SqliteRecordStore`
(`~/.local/share/exponential-desktop/apps/records.sqlite`; `EDE_APPS_DATA_DIR`
overrides), their files next to it, each app's grants in the keyring (or
its `vault.json` without one).

- **A connected team:** when the switcher's current entry is a team of a
  signed-in account, the apps are the TEAM's published packages (the
  account's `apps` shape) over the team's records: the Engine's
  `SyncedRecordStore` (the synced `app_records` as the cache, the account's
  `app_record_outbox` as the queue, tRPC `apps.records.*` out, the same
  providers the IDE uses, `sync::app_outbox`), the viewer the account's
  user with their team role (`team_members`), so every kind policy is the
  team's. A pick of another desktop closes the apps and boots the new set;
  a Local desktop runs the installed packages over the local records.
- **The window:** the start menu lists each app with the installed
  applications; `ede_launch {app: <package id>}` opens its window, a plain
  toplevel of the shell process whose app id is the package id (the
  taskbar groups it under the launcher's entry; `--open-app <id>` opens one
  at start, for the goldens). Closing it leaves the app in the background;
  a widget's press or a notification tap opens it again.
- **Widgets (C26):** a screen with `widget: {template, size: s | m | l}`
  paints that template on its own layer surface on the `bottom` layer
  (over the ground, under the windows), stacked down the top-right of the
  panel at `shell-layout.json` `widgets` (160 px cells, 16 px gap, 16 px
  margin inside the panel's 10): `s` = 160×160, `m` = 336×160, `l` =
  336×336. Its `records:` sources stay live while the window is closed; a
  press that navigates opens the window on that screen.
- **Capabilities:** the desktop's natives (`files`, `notifications`,
  `photos`, `print` over the portals, `eengine-desktop`) and eDE's own
  `windows` (`windows_list`), `focus` (`focus_window`) and `launch`
  (`launch_app`), each an `ask` scope the package's manifest must declare
  (the consent sheet asks once, in the window). They reach the compositor
  over ede-protocol with one `ui` grant minted for the apps, never the
  shell's root token; `launch` starts the launcher's desktop entries.
  `shell.edit` stays host-only: no app can declare it.
- **`shell_edit`, the desktop's own agent:** the chat's local runs (#EDE-58)
  get one more tool on their hub, next to the noun subset: `shell_edit
  {name, document}` writes a shell document after genUI's validation (a
  refused one answers its issues, nothing written), asks the person once
  per run in the sealed consent dialog, then runs `exponential-ui set`,
  so the edit is committed and Version history undoes it. That is the
  host-only `shell.edit` of the catalog: the shell's agent has it, no
  app ever does.
- **The apps' tools on that hub:** every running app's `exposes` list as
  `<app>__<tool>` (the app = its package id with dashes,
  `at-exponential-notes__list_notes`), run as the web's apps broker runs
  them: a `records` tool reads the app's rows through the same records
  query (`list` with a named query and the run's own keys, `get` by id) or
  writes through the instance (`upsert`); an `action` tool runs one of the
  exponential nouns of the Local desktop via the app. `upstream` and
  `agent` runs have no seam on a desktop and stay unlisted; `admin` tools
  never show.
- **The factory apps:** `apps/ede/apps/notes.app.json` (Notes: a records
  kind, an action, a screen with a widget) ships with the package;
  `exponential-shell`'s tests boot it on the Engine.

## The theme bridge: `exponential-theme` (#EDE-19)

Foreign apps look like the shell: `crates/ede-theme` generates, from the
same `packages/design-tokens/tokens.json` the web, the natives, the IDE and
the compositor's own text use, GTK 3 and 4 settings (`settings.ini` and a
`gtk.css` of `@define-color` over the Adwaita and libadwaita names, so
Adwaita-dark and every libadwaita app take the palette: backgrounds,
cards, the white selection, the destructive red), a Qt palette for
qt5ct/qt6ct (`colors/ede.conf`, Fusion; without them Qt's own `gtk3`
platform theme reads the GTK settings), `environment.d/90-ede-theme.conf`
(`XCURSOR_THEME`, `XCURSOR_SIZE`, `EDE_ICON_THEME` for the launcher,
`QT_QPA_PLATFORMTHEME`) and `~/.icons/default/index.theme` for X11
clients, and sets `org.gnome.desktop.interface` to match (libadwaita and
the GTK portal read it). The GTK theme, icon theme, cursor theme and
fonts are the first installed of a short preference list (Adwaita-dark;
Papirus-Dark, Yaru, Adwaita; Bibata, Adwaita; Inter, Ubuntu Sans,
Cantarell), each overridable in `[theme]` (below).

Every file it writes starts with a marker line; `exponential-theme remove` deletes
only those, puts a person's own file back from `*.before-ede` (moved
aside once on the first apply) and restores the previous gsettings
(`$XDG_STATE_HOME/exponential-desktop/theme-before.json`). `exponential-theme sync` applies or
removes as `[theme] bridge` says: exponential-theme.service runs it at login
before the shell, the Theme page's switch runs it after `ede_config`
wrote the key, and exponential-session exports the environment file before the
compositor starts so its cursor matches. `exponential-theme print` shows the
choice and the files; `--files-only` skips the live environment and
gsettings (CI, tests). Nemo stays the file manager and file dialogs stay
GTK through the portal delegate (#EDE-15), both themed through this.

## Config

`$XDG_CONFIG_HOME/exponential-desktop/compositor.toml`, every key optional; a broken file is
logged and ignored.

```toml
background = "#0a0a0a"
xwayland = true       # or --no-xwayland

[keyboard]
layout = "de"         # unset = /etc/default/keyboard's XKBLAYOUT (and variant, options), else xkb's us
repeat_delay = 400
repeat_rate = 30

[windows]
snap_threshold = 12   # px; 0 = off
edge_tiling = true
panel_margin = 10     # the IDE look (#EDE-48): the work area (maximize, tiling, Super+Up/Left/Right, new windows) is the output minus the panel's zone, inset by this many px on every side; 0 = flush

[lock]
command = ["swaylock", "-f", "-c", "000000"]   # any ext-session-lock client; empty = no locking
on_suspend = true     # lock before suspend and on `loginctl lock-session` (udev)

[idle]
lock_after = 600      # seconds without input; 0 = never

[protected]
app_ids = ["org.example.Bank"]   # on top of the built-in list

[theme]              # the bridge (#EDE-19), read by exponential-theme, not the compositor
bridge = true        # false: exponential-theme sync removes what it wrote
icons = "Papirus-Dark"   # gtk, cursor, cursor_size, font, monospace, font_size likewise; unset = the first installed preference

[diagnostics]        # Settings › About › Send diagnostics (#LOGS-2)
send = true          # false (or EXP_DIAGNOSTICS=0): no crash, performance or log reports
```

## Diagnostics (#LOGS-2)

`crates/ede-observability`: the Sentry SDK in the compositor, `exponential-session` and
`exponential-services` (panics, `error!` events, `warn!`/`info!` breadcrumbs and logs, transactions),
OFF unless the signed-in account's server turns it on. Nothing is baked in: the server of the first
account in the IDE's shared `accounts.json` answers `GET <server>/api/monitoring/config?platform=ede`
(`{"enabled", "dsn", "environment", "tracesSampleRate"}`), the answer is cached in
`$XDG_CACHE_HOME/exponential-desktop/monitoring.json` and applies from the NEXT launch (no network
before the desktop starts), and every envelope is POSTed to the server's tunnel
`<server>/api/monitoring`, never to the collector. No account, no cached answer, `{"enabled": false}`,
`[diagnostics] send = false` or `EXP_DIAGNOSTICS=0` = off.

- release `exponential-ede@<version>`; environment = `SENTRY_ENVIRONMENT`, else the server's, else
  `dev` (debug build) / `prod`; traces = `SENTRY_TRACES_SAMPLE_RATE`, else the server's, else 0.1;
  errors 1.0.
- Privacy: `send_default_pii` off, no hostname, the user is the account's pseudonymous id only;
  `Authorization`, cookies and every key matching token|secret|password|key|ticket|session|dsn are
  `[Filtered]` in events, breadcrumbs and logs (`scrub.rs`, unit-tested).
- `EXP_SENTRY_DSN`: a local development override naming the project (still through the tunnel).
- The compositor: every drawn frame is timed; one of 33 ms or more is a `ui.frame.long` transaction,
  a `ui.frames` transaction carries each minute's p50/p95/p99/max, and a heartbeat every 50 ms on the
  main loop turns a block of 100 ms or more into a `ui.hang` transaction (a loop stuck for 2 s is a
  warning event from the watchdog thread). At most one long-frame or hang span a second.
- Symbols: release builds carry line tables, which `packaging/split-debug.sh` moves into
  `<name>.debug` files before packing (the shipped binaries stay as small as before);
  `build-ede.yml` uploads them to GlitchTip on releases (`sentry-cli debug-files upload`).
- `EXP_FRAME_STATS=1`: the same frame times as a stderr summary every 5 s, Sentry or not:
  `frame stats 5.0s: 300 frames (60.0/s) avg 2.10 ms, p50 2.0, p95 4.5, p99 8.0, max 12.3 ms; 0 long (>= 33 ms); 0 hangs (max 0 ms)`.

## Lock, idle, input methods, protected surfaces

The lock screen is a client of `ext-session-lock` (the genUI shell's, later;
`swaylock` today). From the lock request on, every output draws only that
client's lock surface (black until it draws), only it gets keys and clicks,
and bindings are off except Ctrl+Alt+F1…F12. If the lock client crashes the
session stays locked (black) until a new lock client takes over; a second
client can neither lock over it nor unlock it. Super+L, `[idle] lock_after`
(held off by idle inhibitors such as a playing video), `loginctl
lock-session` and suspend run `[lock] command`; before suspend a logind delay
inhibitor waits up to 2 s for the lock to show. `ext-idle-notify` is there for
the shell's screen blanking.

Protected surfaces are never captured, lent or driven: the lock screen,
polkit and keyring prompts, pinentry, ssh askpass and password managers
(`ede_compositor::protected::BUILTIN`, matched by app id, WM_CLASS or the
process's executable), plus `[protected] app_ids`. Captures draw them black
(`backend::capture_elements`; a locked session is black altogether), agent
input to them is refused, a virtual keyboard cannot type into them, and an
input method loses its keyboard grab and stays off while one has focus.

Try it on real hardware (Danny; never from a session another display server
owns): `sudo apt install swaylock`, put the `[lock]` block above in the
config, log into **Exponential**, then check: Super+L locks and the password
unlocks; `loginctl lock-session` locks; `systemctl suspend` and wake shows
the lock screen, never the desktop; `pkill -9 swaylock` while locked leaves a
black screen, and `swaylock` from a VT (`WAYLAND_DISPLAY=wayland-1
XDG_RUNTIME_DIR=/run/user/$UID swaylock`) takes over; a video in Firefox
holds off the idle lock; fcitx5 or ibus types into a GTK app but not into a
polkit prompt (`pkexec true`).

## genUI on disk (exponential-ui)

`$XDG_CONFIG_HOME/exponential-desktop/ui/` (default `~/.config/exponential-desktop/ui/`; `$EDE_UI_DIR` wins)
holds the shell: one genUI surface per `<name>.json` (lowercase, digits, `-`,
`_`) and the look in `theme.json` (the theme format of the SDK; `extends` may
name a built-in theme; none = `exponential`). The factory copies live in
`/usr/share/exponential-desktop/ui` (`$EDE_FACTORY_DIR`; `apps/ede/ui` in this repo): a fresh
account gets them as the first commits, and a rollback always has somewhere
to go. The contract is `packages/ede` (#EDE-8): its fixtures are the spec and
`crates/ede-ui/tests/contract.rs` replays them.

```json
{
  "surface": {"layer": "top", "anchor": ["top", "left", "right"], "size": {"height": 40}, "exclusiveZone": 40},
  "root": {"id": "panel", "component": "Box", "style": {"display": "flex"},
           "children": [{"id": "clock", "component": "Text", "props": {"text": {"@path": "/time"}}}]},
  "data": {"time": "12:00"}
}
```

`surface` says where the shell puts it: a layer surface (`layer`, `anchor`,
`size`, `exclusiveZone`, `margin`, `keyboard`) or `popup` (a popup of another
document under one of its components, drawn as an `xdg_popup` so it can leave
the parent's box); `functions` lists the host functions the document may
call. `root` is the nested authoring form; `components` (A2UI's flat
`updateComponents` list, from the component `root`) works too; `catalogId`
defaults to the core. Tier C (a document grants no privileges): no password
inputs, no `functionCall` outside `surface.functions`, ids once; then
eui-core's reducer validates every component, prop, style and token. One
issue refuses the whole file and the last good version stays on screen
(after a restart, the last committed one). The directory is a git repository
(the git CLI; created on first open): every edit that applies is a commit, an
edit through the API with its own message, a hand edit as `Edit <files>` once
it validates; refused files are never committed. Undo reverts the newest
commit as a new commit, so an undo is undoable too.

```bash
cargo run -p ede-ui -- check                    # what applies, what is refused and why
cargo run -p ede-ui -- set panel panel.json     # validate, write, commit
cargo run -p ede-ui -- undo                     # revert the newest edit
cargo run -p ede-ui -- discard panel            # drop a broken hand edit
cargo run -p ede-ui -- log
cargo run -p ede-ui -- watch                    # hot reload, one JSON change per line
cargo run -p ede-ui -- supervise -- exponential-shell   # run the shell under the watchdog
```

**The supervisor** (`exponential-ui supervise -- <shell command>`) owns the history
and runs the apply machine of `packages/ede/fixtures/apply.json`: it starts
the shell with `EDE_UI_DIR`, commits hand edits, refuses broken ones, and
expects the shell to print `{"frame": {"name": "panel", "digest": "…"}}`
(`exponential-ui digest FILE`, the content digest) on stdout once it drew a document.
No frame within 2 s of an edit = the edit is reverted (`Undo: Edit
panel.json, rolled back`); the shell dying counts as a crash, it restarts
after 500 ms, and three crashes after an edit revert it, three more put the
factory copy back (`Factory panel`). The shell itself opens the directory
read-only (`Runtime::open_readonly`): it validates and reloads but never
writes history.

```rust
let (ui, _) = ede_ui::Runtime::open_readonly(ede_ui::default_dir())?;
let ui = std::sync::Arc::new(std::sync::Mutex::new(ui));
let _w = ede_ui::watch(ui.clone(), |changes| for c in changes { println!("{}", c.kind()) })?;
// ui.lock().document("panel") → .surface (where it goes) and .surface(theme) → an eui-core Surface to lay out and paint
```

Changes: `ui.applied`, `ui.rejected` (issues, whether a last good version
stays), `ui.removed`, `theme.applied`, `theme.rejected`. Try it: `exponential-ui
--factory apps/ede/ui check` on a fresh directory creates it with the
factory commits; `exponential-ui watch` in one terminal, edit `panel.json` in an
editor (gedit, vim: both save through a rename) and see `ui.applied`, break
it and see `ui.rejected` with `"fallback": true`, then `exponential-ui undo` and
`git -C ~/.config/exponential-desktop/ui log`.

## Tests

```bash
cargo test --workspace   # headless: real clients, real input, the pixels pixman drew
                         # tests/lock.rs: a placeholder lock client, idle, protected windows
```

Build deps: `libxkbcommon-dev libpixman-1-dev libwayland-dev pkg-config`
(`xwayland` + `libegl1` at runtime and for the Xwayland test; Ubuntu's xwayland
package does not pull in libEGL, and Xwayland aborts without it);
the `udev` feature adds `libudev-dev libinput-dev libgbm-dev libseat-dev
libdrm-dev`.
