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
A press follows the spec (#EDE-196): an action button sends the app
`ActionInvoked(id, key)`, a press on the toast or the bell's row its
`default` action (never drawn as a button) when it gave one; then the
notification closes (`NotificationClosed(id, 2)`) unless the app marked
it `resident`. Host functions: `ede_notify_open {id}`, `ede_notify_close
{id}`, `ede_notify_action {id, key}` (each action in `/notifications`
carries its row's `id`), `ede_notify_clear`, `ede_tray {service}`,
`ede_tray_menu {service, id}`.

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
root grant only, on the person's desk alone), which is how the goldens and an agent in the VM drive
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
Photograph it only with `scripts/lanes/capture.sh :77 <out.png>` (#EXP-2022),
which refuses the live display.

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
  starts it on Super+L, idle, suspend, `loginctl lock-session` and the
  `lock` op (Lock in the start menu); while locked only that process's
  surfaces show (black until one arrives), only they get input, bindings
  are off except VT switching. The password goes to PAM inside the binary
  (`libpam.so.0` opened at run time; service `ede` when `/etc/pam.d/exponential-desktop`
  exists, else `login`), and the session opens only through the `unlock`
  op with the grant the compositor minted for that process, handed over
  its stdin (`EDE_TOKEN_FD`), never its environment. A lock client that
  dies or shows nothing keeps the session locked and is started again;
  when its display goes away the prompt moves to the next one.
  `swaylock` still works through `[lock] command` (ext-session-lock).
- **polkit** (`exponential-sealed polkit`, `exponential-polkit-agent.service`): the
  session's authentication agent; polkitd's request opens the prompt, the
  password goes to `polkit-agent-helper-1`. A request whose subject or
  caller belongs to an agent space is refused before anything maps (#EDE-127:
  no prompt, the person's keyboard stays where it is, the space's command
  fails; journal: `refused a polkit request from agent space N: <action>`).
  Whose a process is: its cgroup first (#EDE-138: a scope of a space's
  systemd slice, which a process keeps when it starts a session of its own
  with an empty environment and lets its parent die), then its environment
  or an ancestor's naming the space's directory, else the compositor's
  `space_of_process` (what it launched into the space, that launch's
  process group, the space's socket's clients).
  Everything else of the user is one of two kinds (#EDE-143). A process in
  the person's login session (the compositor's own cgroup, root's, which
  no space's process gets back into) gets the prompt as always: it takes
  the keyboard. A process anywhere else gets a prompt that shows WITHOUT
  taking the keyboard and names it ("Asked by <command line> (process
  N)"): the person's user services, snaps, Flatpaks, a coding session's
  shell, or a space's process that moved itself out of its slice. The
  person clicks into the prompt to type. While a space has launches
  outside systemd scopes (no user manager), they are in the session's
  cgroup too, so every request is of the second kind. The second kind
  exists only while an agent space does (one is open, a closed one's slice
  is still stopping, or a launch of one is alive): with no space, before
  the first and after the last one closed, every prompt takes the keyboard
  as before #EDE-143.
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
every 250 ms while the window is dragged). After any change (`set_output`,
such a resize, an unplug) the outputs without a configured or
`set_output` position close ranks, layers re-arrange and every window
stays on its output (#EDE-139): it goes along as the output moves, tiled
and fullscreen windows fill it again, one it no longer holds comes back
inside it, and the windows of an output that went away reopen on the
first one (a fullscreen one fullscreen there). Nothing is raised. In VirtualBox this is the guest additions' resize: `VBoxDRMClient`
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
switches back. A parked or minimized window comes back fitted to its output
as it is now (a tiled or fullscreen one fills it; one whose output went
away opens on the first, #EDE-139). Over ede-protocol: `list_workspaces`, `switch_workspace`,
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
| `list_windows`, `list_outputs`, `pointer_position`, `subscribe` (events: `window.opened/closed/focused/changed/minimized/restored`, `session.locked/unlocked`, `config.reloaded`; #EDE-91: a window, an output and their events may name a `space`, absent = the person's desk, which is all the shell, ePilot and screen memory read; nothing sets it yet) | any grant |
| `focus_window` (a minimized window comes back first, a parked one brings its workspace along), `close_window`, `move_window`, `minimize_window`, `restore_window` (#EDE-48: into the taskbar and back; a minimized window keeps its geometry and workspace, `WindowInfo.minimized`; `exponential-ctl minimize <id>` / `restore <id>`) | `ui` |
| `capture` (an output: protected windows black, locked = black; a window: protected ones refused) | `capture` |
| `input` (key by evdev code, `keysym` = the key that types it on the active layout with its Shift/AltGr held around it, pointer motion, button, wheel; refused on protected surfaces and the lock) | `input:person` |
| `hold_input`, `release_input` (#EDE-23: the person's keys, pointer and wheel reach no window while the connection lives, the agent's `input` still does; their Esc ends every hold and reaches the holders as `input.escape`; the lock screen always takes their input) | `input:person` |
| `reload_config`, `lock`, `mint_grant`, `revoke_grant` | `system` |
| `space_of_process {pid}` (#EDE-127: the agent space a process belongs to, `{space: {space, by, pid} or null, session, unscoped_launches, no_space}` with `by` = `cgroup` (#EDE-138, looked at first), `launch`, `group`, `client` or `environment`; #EDE-143: `session` = it runs in the compositor's own cgroup, `unscoped_launches` = a space has launches outside systemd scopes, `no_space` = no agent space exists (none open, none stopping, no launch alive); the polkit agent asks it) | `system` |
| `space_busy` (#EXP-1514, D 10: the caller's space does not give way to a new one for 90 s; ePilot's keeper says it each round while the session's turn runs) | a space grant, its own space |
| `space_readiness` (#EXP-1514, #EXP-2011: `{ready, block?, max?}`, whether a person's runs get an agent space now; every compositor that makes spaces says `space-readiness` and answers from what holds now: `no_user_manager` (its tools, its private socket, memory and pids delegated to it), `guard` (the space guard installed and answering, polkit refusing a space's process), `session` (a space's session failed to start, until one starts); see "Agent spaces: readiness"; never a space grant's) | any person-scope grant |
| `launch_token` (#EDE-76: a one-shot token, good for 120 s, for the `XDG_ACTIVATION_TOKEN` and `DESKTOP_STARTUP_ID` of a launch the person made; that app's window takes the keyboard, by its process or over xdg-activation; any other new window opens behind the focused one) | the shell's root grant only |

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
lending come with their own issues; a space's captures are drawn on its GPU when it has one
(below), the person's on the backend's renderer.

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
cargo test -p epilot headless_ede` (in `apps/desktop`); the gate's `ede` job
runs it on every PR that touches eDE or ePilot (#EDE-140).

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
Papirus-Dark, Yaru-dark, Yaru, Adwaita; Bibata, Adwaita; Inter, Ubuntu
Sans, Cantarell), each overridable in `[theme]` (below). With snapd
(#EDE-196) a theme the installed snaps carry too comes first
(`gtk-common-themes`: Yaru-dark, Adwaita, …, read from snapd's
`/v2/connections`): snap apps draw with the same names, and
snapd-desktop-integration no longer asks at every login to install a
Papirus theme snap.

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

## Agent spaces: a systemd slice per space (#EDE-138)

What an agent launches into a space runs in a systemd scope of the space's
own slice, in the person's user manager:

```
app.slice/app-exponential_p<compositor pid>.slice    all spaces: CPU quota, memory, oomd
  └─ app-exponential_p<pid>-s<n>.slice               space n: memory, tasks
       └─ exponential-p<pid>-s<n>-<k>.scope          launch k
```

- **Out of the login session.** The person's apps stay in their login
  session scope; a space's processes are in none.
- **Limits, the person's to set** (`[spaces]` below, read at a space's
  first launch): all spaces together half of the cores and half of the
  memory by default, one space 30 % of the memory. `systemctl --user status
  'app-exponential_p*.slice'` shows them. The CPU limit is a quota: the
  person's apps are in a sibling of the whole user manager, so a weight
  inside it could not keep spaces below half of a busy machine.
- **How many, and the memory they share.** `[spaces] max` spaces at once
  (unset: one per GiB of what all spaces may use: 8 GB of RAM = 4, 16 GB =
  8, 64 GB = 31, at most 32), `[spaces] launches` programs each (32). The
  spaces SHARE their memory: one may take up to its 30 % while the others
  are still, so together they are promised more than the 50 % they have.
  The fence is room: a new space is made only while what the spaces hold
  (anonymous and shared memory of their slice) plus one still space (1 GiB:
  a browser page measured 0.5 to 0.9 GB) fits under 90 % of their
  `MemoryMax`.
- **Over its limit, a space loses its biggest program** (#EDE-192). A
  space's `MemoryMax` is where the kernel ends a program, inside the
  space's own slice, at once, whether or not systemd-oomd runs. No slice
  has a `MemoryHigh`: under it the kernel throttled a space just short of
  its `MemoryMax` and ended nothing (only systemd-oomd did, 39 s later, by
  the pressure of the person's whole user manager). No space swaps: with
  swap a space at its limit first writes out what it may swap and stalls
  while the disk is busy (15 s measured), and its swap is the person's;
  an idle space stays in RAM, within its `MemoryMax`. The kernel ends one
  program per OOM, the biggest (by its `oom_score_adj` too):
  `memory.oom.group` stays 0, and from systemd 253 a launch's scope has
  `OOMPolicy=continue`, so the launch's other programs and the other
  spaces run on (systemd 252 ends the whole scope: the launch, or the
  space's own user manager with all it started). An app the space's
  own user manager starts in a scope of its own (a snap, a flatpak)
  keeps that manager's default (`OOMPolicy=stop`): when the kernel picks
  one of its programs, the whole app ends. What a space keeps in tmpfs
  (`/dev/shm`, a tmpfs `/tmp`) no kill frees. Together the spaces are
  held the same way at their `MemoryMax`, the biggest program of any
  space first. A space that thrashes on its programs' files short of a
  kill is systemd-oomd's, where it runs: it watches the slice of all
  spaces at 30 % memory pressure for its usual time
  (`ManagedOOMMemoryPressure=kill`), below the 50 % at which Ubuntu and
  Fedora watch the person's whole user manager, and ends the scope that
  reclaims most before it looks at the person's services.
- **A full desktop gives way** (#EXP-1514, D 10). A space lives as long
  as its session, so when a NEW one does not fit (the count or the room),
  the space unused longest closes for it: one neither driven nor captured
  by its own grant for 10 minutes (a space never used counts from its
  creation) and not busy (its keeper said `space_busy` in the last 90 s:
  its session's turn runs). For room as many give way as it takes, what
  each holds read from its own slice, planned before one closes. Only
  when none may does `create_space` answer `limit` (`N spaces at most`,
  `no memory for another space: …`), and then none closed. A session's own
  space is always found again first.
- **Grants and the clock.** An expired grant leaves the list in the 1 s
  tick. A space whose owner holds no live space grant closes after 30
  ticks; after the wall clock jumped ahead of the ticks (a suspend) no space
  closes for 5 minutes, one space grant's life, so its keeper can renew.
  The log and the doctor's line say the numbers. Without a user manager:
  4 spaces and 32 launches in all, as before.
- **Homes on disk.** A launch's `XDG_CONFIG_HOME`, `XDG_CACHE_HOME`,
  `XDG_DATA_HOME`, `XDG_STATE_HOME` and `GNUPGHOME` are in
  `$XDG_CACHE_HOME/exponential-desktop/spaces/p<compositor pid>/space-<n>/`
  (a cache: no backup wants an agent's browser profile), removed at the
  close; the next compositor's start removes what a gone one left. Only
  sockets and the bus are in the runtime directory, a tmpfs the person's
  session shares.
- **The environment is an allowlist**: of the compositor's, a launch gets
  who the person is, the language, `PATH`, the desktop's name and the
  toolkits' hints, nothing else (no `SSH_AUTH_SOCK`, `TMUX`, `STY`,
  `NOTIFY_SOCKET`, `DOCKER_HOST`, keyring or gpg-agent variables). It gets
  a tmux server (`TMUX_TMPDIR`) and screen sessions (`SCREENDIR`) of the
  space's own: an agent's `tmux` never opens in the person's server.
- **The close ends everything.** The space's slice is stopped (SIGTERM,
  SIGKILL two seconds later), then the kernel ends what is left in its
  cgroup (`cgroup.kill`): a process that started a session of its own, a
  scope that asked not to be killed, a launch that was still on its way in.
  The compositor never waits for the user manager in its event loop.
- **The compositor's end ends everything.** With lingering on (the
  Exponential daemon turns it on) the user manager outlives the person's
  logout, and nothing else would stop what a space left. A compositor that
  ends by itself kills what is in the slice of all its spaces and waits (3 s
  at most) until the user manager has taken the slice away. One that is
  ended from outside (the session's SIGTERM, a crash) stops nothing:
  `exponential-session` then runs `exponential-compositor space-end <pid>`,
  which does the same for it. What is still left (the session supervisor
  died too) is stopped at the next compositor's start.
- **How.** A launch is four programs in one process: `exponential-compositor
  space-run` makes the slices when the space has none (`busctl`), becomes
  `systemd-run --user --scope`; in the scope `space-enter` is the first
  program (the entry of every scope into a space: it asks the space guard
  below, then becomes the next), and `space-exec` takes the
  caller's environment from its standard input and becomes the program. No
  D-Bus library in the compositor; nothing the caller chose (a variable, an
  `LD_PRELOAD`) reaches a program outside the scope or an argument. A launch
  that gets no scope does not run, and a compositor that found its user
  manager never starts one outside a scope: when the user manager is gone
  later, `launch` answers `unavailable`.
- **Needs** a systemd user manager on the session bus, systemd **252** or
  newer (the first that hands the cpu controller to the user manager), a
  unified cgroup hierarchy (cgroup v2), and `systemd-run`, `busctl` and
  `systemctl`. Without one of them (Ubuntu 22.04 has systemd 249, a
  container has no user manager) spaces work as before for the whole run:
  launches run in the login session, without limits, and end by their
  process group. The log says so once at the session's start, and
  `exponential-compositor doctor` asks the same questions for its line
  `Agent spaces`.
- **The space's own user manager and bus.** At a space's first launch the
  compositor starts `systemd --user` for the space, in a `Delegate=yes`
  scope of its slice (`exponential-p<pid>-s<n>-session<k>.scope`): the
  space's directory is its runtime directory, and it can load only the
  units written into `<space dir>/units` (its bus, nothing of the
  distribution's or of `~/.config/systemd/user`). Its `dbus.service` is the
  space's ONE session bus (`<space dir>/bus` and an abstract
  `/tmp/dbus-ede-<random>`, which strict snaps can reach), with what it
  activates (dconf, gvfs, portals with the GTK backend; screen capture,
  remote control, shortcuts, notifications and inhibit are `none` in the
  space). Every launch of the space gets that bus and waits for it in its
  own process. So `snap run`, Ptyxis, `systemd-run --user` and anything
  that asks "the" user manager for a scope find the space's, and their
  scopes are in the space's slice. A manager that ends is started once
  more; when it ends again the space closes (its session's next call reads
  "new and empty"). Needs `dbus-daemon` (the packages depend on it).
  A Flatpak installed for the person alone (`flatpak install --user`) is
  not there in a space: its installation is under `XDG_DATA_HOME`, and a
  space has its own; system-wide ones are (#EDE-158). `flatpak run` asks
  the person's user manager for its scope whatever the environment says,
  so the compositor finds such a scope of a space's (its first process is
  a launch of the space's, or a child of one) within a second and moves
  its processes into a scope of the space's own manager. They are then
  under the space's limits and end with it, and what is still outside at
  the close is ended by the kernel.
- **Drawing in a space: on the CPU, unless the space draws on the GPU**
  (#EDE-162, [below](#agent-spaces-on-the-gpu-ede-162)). A space in
  software takes shared-memory buffers only, so a frame drawn on the GPU
  cannot reach it. OpenGL and EGL apps get Mesa's software driver
  (llvmpipe) by themselves. Vulkan apps would pick the GPU and then fail,
  so while a space renders in software its programs see lavapipe, Mesa's
  software Vulkan, alone (`VK_ICD_FILENAMES`, which Chromium's SwiftShader
  may replace; a launch may set neither it nor `VK_DRIVER_FILES`). The
  packages recommend `mesa-vulkan-drivers` (Arch:
  `vulkan-swrast`); without it a Vulkan app does not start in a space, and
  the doctor says so. The cost: 3D apps and games are slow or do not run
  in a space, and every frame is drawn on the processor. A WebGL page or a
  spinning 3D view keeps one or more cores busy; the space's share of the
  processor (`[spaces] cpu_quota`) caps it.
- **An Xwayland of its own.** X11 apps launched into a space get
  `DISPLAY=:<10 or more>` (never `:0`): an Xwayland started at the space's
  first launch, whose Wayland client belongs to the space's desk (it binds
  the space's seat and output only) and whose window manager puts its
  windows on the space's desk; the person's X server and window manager
  are untouched. One that dies is started again at the space's next
  launch; it ends with the space (its socket and lock go). Its CLIPBOARD
  is the space's: an X11 copy there is the space's selection (the agent
  reads it, the space's Wayland apps paste it), and a copy by the space's
  Wayland apps or by the agent pastes in its X11 apps. Only the X11 app
  holding the space's keyboard reads it, and the person's clipboard and
  X11 CLIPBOARD are never involved. A drag the space's X11 app starts runs
  on the space's seat. A space that closes in the middle of a paste ends
  it there (an agent's read is answered `unavailable`).
- **X authority, both sides.** Every Xwayland of the compositor admits
  only a client that shows its cookie (MIT-MAGIC-COOKIE-1). The person's
  file is `$XDG_RUNTIME_DIR/xauth_<wayland socket>`. Its path stays the
  same for the compositor's life, and it reaches every way the person's
  apps start: the compositor's children (the shell's launches among them)
  get `XAUTHORITY`, and `exponential-session --ready` hands it to the user
  manager and D-Bus activation with `DISPLAY` (terminals and services
  inherit it from there). A space's file is
  `$XDG_RUNTIME_DIR/xauth_<wayland socket>-space-<n>`, with a new cookie
  for each of its servers; only its launches get it, and it goes when the
  space closes. The `xauth_` names are the ones snaps can read: `snap run`
  copies only an `XAUTHORITY` under `/tmp`, and the x11 interface lets a
  snap read `/run/user/<uid>/xauth_*`. So `DISPLAY=:0 xdotool type …` from a
  space is refused by the person's server, and the person's apps cannot
  reach a space's either. A snap app in a space that the space's Wayland
  socket refuses (its AppArmor admits only `/run/user/<uid>/wayland-N`)
  draws through the space's Xwayland.
- **Browsers get the space's profile.** Snaps and Flatpaks keep their data
  under the person's `HOME`, which a space's own directories do not
  change. Without a profile of the space, its Chromium would hand its
  window to the person's running one (a window on the person's desk), or
  browse in the person's signed-in profile, and Firefox would say "already
  running". So a known browser started in a space gets a profile of its
  own. The Chromium family (Chromium, Chrome, Brave, Edge, Vivaldi, and
  their binaries started by name: `chrome` of Chrome, Chrome for Testing
  and Playwright, `chrome-headless-shell`, `headless_shell`, `msedge`,
  `vivaldi-bin`) gets `--user-data-dir=`; Firefox and LibreWolf get
  `--profile <dir> --new-instance`. For a browser of the system the
  directory is in the space's home. For a snap or a Flatpak (its command,
  `snap run`, `flatpak run`, or Ubuntu's `/usr/bin/firefox` script that
  runs the snap) it is in the app's own data, the one place it may write:
  `~/snap/<name>/common/exponential-space/p<pid>-<space>` or
  `~/.var/app/<id>/exponential-space/p<pid>-<space>`. It is removed when
  the space closes, and what a crashed session left there goes at the next
  start. A launch that names its own profile (Playwright always does, in a
  temporary directory) keeps it, and the space makes none, unless that
  profile is, or is inside, one of the person's:
  `~/.config/{chromium,google-chrome*,BraveSoftware,microsoft-edge*,vivaldi*,mozilla,librewolf}`
  (and the same under `$XDG_CONFIG_HOME`), `~/.mozilla`, `~/.librewolf`, or
  a known browser's `~/snap/<name>` or `~/.var/app/<id>` outside its
  `exponential-space`, each followed through links and `..` from the
  launch's directory. Such a launch is refused (#EDE-186), and so are
  Firefox's `-P`, `-ProfileManager` and `-CreateProfile` (they go to the
  person's `profiles.ini`), a profile flag that names no directory, and a
  Chromium `--profile-directory=` that is a path, not a name.
  Chromium and Electron apps (VS Code, VSCodium) also always get
  `--enable-unsafe-swiftshader`, so their WebGL can fall back to software,
  and `--use-angle=swiftshader` while the space renders in software, with
  their own profile as with the space's. A snap
  of the Chromium family also gets `--ozone-platform=x11`: it exits where
  the space's Wayland socket is refused, rather than falling back. Firefox
  gets nothing more: it switches to software WebRender by itself. A browser
  that is not on the list runs like any other program. A browser of the
  system then keeps its data in the space's own directories, but a snap or
  Flatpak one uses the person's profile and may open on the person's desk
  (#EDE-166).
- **The person's Xwayland starts again** when it dies, on the same display
  and with the same cookie file, so the `DISPLAY` and `XAUTHORITY` the
  session was given stay right. Its X11 windows are let go of, and its
  X11 copy goes too. It waits 1 s after the first death, and 2, 4, 8, 16,
  30, then 60 s after each further death of a server that ran less than
  a minute. After 8 such deaths in a row it is not started again, and X11
  apps return at the next login. Each death is logged as an error, which
  diagnostics report.
- **Not a wall.** Everything in a space is the person's user. Every cgroup
  under the user manager belongs to that user: a process that works at it
  raises its own limits or moves itself into another cgroup. The slice
  guards against the accidents of ordinary programs (#EDE-145).
- **The close also ends what got out** (#EDE-183). A close ends the slice
  and the launches' process groups. It also ends every process the
  compositor ties to the space some other way: it descends from one of the
  space's launches, or the space's directory is in its environment. That
  holds wherever the process's cgroup is now, and also for a process that
  left its launch's group. Each one is told to end, and killed after 2 s.
  Until then a space still counts as there for the polkit prompt's class
  (#EDE-143), so such a process never gets the person's keyboard prompt. A
  process that cleared its environment, left its launch's lineage and
  group, and left the slice is tied to nothing (#EDE-145).

## The space guard: `exponential-space-guard` (#EDE-146, #EDE-147, #EDE-148)

A space's programs are the person's user, so by user they may open the
person's microphone and camera (the `uaccess` ACLs logind gives the person,
the audio and video groups), switch their radios off (`/dev/rfkill`) or type
into their session (`/dev/uinput`). What tells a space's processes apart is
their slice (above). The space guard is the one privileged part of eDE that
acts on it: when a process INSIDE a space's slice asks, it attaches a fixed
eBPF device program to that slice. From then on the kernel refuses every
`open()` and `mknod()` by a process of the space of:

| devices | why |
|---|---|
| `/dev/snd/*` (116), OSS (14) | the microphone (and the speakers) |
| `/dev/video*`, `/dev/v4l-subdev*` (81), DVB (212) | cameras, libcamera, video capture |
| `/dev/input/*` (13), `/dev/uinput` (10:223) | input devices; `ydotool` |
| `/dev/hidraw*`, `/dev/usbmon*` (their majors read from `/proc/devices` when the guard starts) | HID reports (keyboards with a vendor's udev rule, security keys), USB traffic |
| `/dev/rfkill` (10:242) for writing | `rfkill block all` turns Wi-Fi and Bluetooth off |
| `/dev/i2c-*` (89) | `ddcutil` switches monitors off |
| `/dev/bus/usb/*` (189) for writing | PTP cameras, libusb capture |

whoever owns the node and whatever its ACL says, also for root in the
space, for a snap with its `camera` interface, a reopen through
`/proc/<pid>/fd`. Everything else (the GPU, ttys, fuse, `/dev/null`) is as
before; WebAuthn keys, game controllers, adb and WebUSB do not work in a
space. The person's own processes are not affected. The table is
`crates/ede-space-guard/src/devices.rs`; the program is generated from it
and tested in an eBPF interpreter on every device.

With it the guard attaches a connect program, for `connect()` and for a
datagram's `sendto()`, which refuses a unix socket made in the space the
way to (`crates/ede-space-guard/src/connect.rs`):

| path | why |
|---|---|
| `/run/dbus/system_bus_socket` | no system bus in a space: no `loginctl lock-session` or `terminate-session`, no `systemctl suspend`/`reboot`, no `nmcli networking off`, no `udisksctl mount`, no `pkexec` |
| `/run/user/<uid>/bus`, `/run/user/<uid>/.dbus-proxy/…` | the person's session bus and their Flatpaks' proxies (their portals hand out camera streams) |
| `/run/user/<uid>/systemd/…` | the person's user manager: nothing leaves the slice through it (also not `flatpak run`'s hard-coded socket: the Flatpak stays in its launch's scope) |
| `/run/user/<uid>/exponential-desktop/…` (but the spaces' own directory) | eDE's control socket (lock, log out with the root token) and `services.sock` (power, network) |
| `/run/systemd/private`, `/run/systemd/io.systemd.*` | PID 1 for root (`sudo reboot`), logind's Varlink (`sudo -i` would get a new login session) |
| `/tmp/tmux-<uid>/…`, `/run/screen/…` | the person's tmux and screen servers run what they are sent outside the space |
| `/run/wpa_supplicant/…`, `/run/hostapd/…`, `/run/tailscale/…`, `/run/mullvad-vpn`, `/run/cloudflare-warp/…` | `wpa_cli disconnect` (a member of `netdev`), `tailscale down` and the like |
| `/run/docker.sock`, `/run/user/<uid>/docker.sock`, `/run/libvirt/…`, `/run/incus/…`, `/var/lib/incus/…`, `/var/lib/lxd/…`, `/var/snap/lxd/…` | daemons that act for their group as root, and a rootless docker of the person's |
| `/run/…/pulse/native`, `/run/…/pipewire-<n>[-manager]`, `/dev/shm/jack…` | the person's sound servers: the microphone (a space has no speakers either), also as a snap's launch helper spells them (`/run/user/<uid>/snap.<name>/../pulse/native`) |
| `/run/…/<wayland-N>` | the person's Wayland socket (#EDE-145): a program that names it (`WAYLAND_DISPLAY=/run/user/<uid>/wayland-1`) opens its windows on the person's desk. Its name is the one the directory of spaces carries (`<wayland-N>.spaces`, which `enter` hands the guard), and every path under `/run` ending in it is refused: also a snap's link to it (`/run/user/<uid>/snap.<name>/wayland-N`, which the snap's desktop launcher makes) and a Flatpak's bind of it (`/run/flatpak/wayland-N`). A space's own socket is in its directory, and eDE's socket is never `wayland-0`, the name a space's Flatpak gives the space's own |
| any path with `/../` | so no other rule is walked around by spelling |

It reads the address as the caller passed it, bounded by its length
(libdbus passes no NUL), before the kernel looks the path up; an address
whose family is not AF_UNIX is refused (a misread layout fails closed). A
symlink of the space's own, a relative path or `//` get around it, on
purpose. Abstract sockets (the space's bus) are not paths and pass, and
so does the person's Xwayland's abstract socket: until Xwayland gets an
X authority (F3), `DISPLAY=:0 xdotool type …` from a space reaches the
person's X11 apps.

The package also ships `/usr/share/polkit-1/rules.d/00-00-exponential-agent-spaces.rules`:
polkit answers NO for every action whose subject is in a space's slice
(polkit takes a space's processes for the person's active session, so
suspend, reboot, network and mounts would pass without a prompt). It reads
the subject's `/proc/<pid>/cgroup` with `cat`: about 4 ms per polkit check
of a process of the person's user manager (measured on Ubuntu 26.04), and
polkitd checks one at a time, so a flood of such checks slows every
authorization on the machine. It never answers YES; when polkitd cannot
run `cat` at all, it asks for an administrator's password instead of
passing. An earlier rule that answers wins (polkit sorts all rule files
by name: hence `00-00-`). logind's `Lock`/`Terminate` for the session's
owner never ask polkit: the connect program closes those. The session
checks that the rule is read: a probe in its spaces' slice asks for the
package's action `at.exponential.desktop.space-probe`, which the rule
answers with a challenge; readiness and the doctor's line need it.

- **What it is.** `/usr/libexec/exponential-space-guard`, a system service
  started by `exponential-space-guard.socket` (`/run/exponential-space-guard.sock`;
  the package links it into `sockets.target.wants`, so it is `static`:
  `systemctl mask exponential-space-guard.socket` turns it off). It ends
  after 10 minutes without a request.
- **Fail closed.** When the guard is installed, every launch into a space
  runs `exponential-compositor space-enter --guard` first, inside its scope:
  it asks the guard (`enter <spaces>`) and becomes the program only on `ok`.
  A guard that is busy, down or broken means the launch does not run;
  without a user manager or with the guard's socket gone, `launch` answers
  `unavailable`. Nothing of that depends on what the guard answered at the
  session's start: the session only asks it for readiness and the doctor,
  which reads what the running session decided (`<wayland-N>.guard` beside
  its socket). Uninstalled, launches run as before, and the doctor says so.
- **A guard from before** (#EDE-145). The session's probe asks
  `probe wayland`. A guard from before the person's Wayland socket was
  refused answers it `not a request`; the session then asks `probe`, and
  the doctor names the person's Wayland socket among what is open in a
  space (its state file says `wayland open`).
- **Its three capabilities, no root.** A user of its own (`DynamicUser`),
  `CAP_BPF` and `CAP_NET_ADMIN` (to load and attach cgroup programs),
  `CAP_PERFMON` (the connect program reads the address it is asked about),
  nothing else; no network, no home directories, the system read-only, a
  narrow syscall set. What a compromised guard could do with them: attach
  cgroup programs to any cgroup (also refusing devices or sockets to other
  users' processes) and, with `CAP_BPF` and `CAP_PERFMON`, load tracing
  programs that read kernel and process memory.
- **What a hostile caller can make it do.** Anyone on the machine can ask.
  The guard takes the asking process's user and cgroup from the kernel
  (`SO_PEERCRED`, `/proc/<pid>/cgroup`), requires that cgroup to be inside
  a space slice (`app-exponential_p<pid>-s<n>.slice`) under THAT user's own
  manager and owned by them, and attaches its fixed programs there. So the
  most a caller can do is restrict cgroups of its own user: take sound,
  cameras and input devices away from its own processes. It writes no file,
  never detaches anything and touches no other user. A space's programs
  cannot undo it: the person's user can neither load a program nor get a
  descriptor of the guard's, and without one the kernel refuses to detach
  programs attached with `BPF_F_ALLOW_MULTI`. No user can starve another:
  each user has two requests in the guard's hands at most (users without a
  runtime directory, such as a user namespace's subordinate ids, share
  two), each request gets one second from connect to its line, and
  `space-enter` asks again after `busy` within its 5 s. At most 64 live
  space slices per user carry its programs, and at most 4 directories of
  spaces have connect programs; a removed slice frees its place.
- **Not a wall** (#EDE-145). Not stopped by it: a process that moves itself
  to another cgroup of the user; a descriptor opened outside the space and
  passed in; the person's terminals (`/dev/pts/*`: a space can read what
  the person types into a shell, or print a fake prompt there, as any
  program of the person's can); where `kernel.yama.ptrace_scope` is 0, a
  space taking descriptors from the person's processes (`pidfd_getfd`);
  root in a space (sudo with the person's password) detaching the program.
- **Needs** cgroup v2, Linux 6.7 or newer (the unix connect hook) with BTF,
  lockdown below `confidentiality`, and a kernel that loads both programs
  (`exponential-space-guard probe` prints `ok` or why not).

## Agent spaces: readiness (#EXP-1514, #EXP-2011)

eDE 0.2.29 and before say `spaces` in their hello and nothing else: no
device offers a person's runs a space there. From the flip on every
compositor that makes spaces also says `space-readiness`, and
`space_readiness` is what offers a space to people (ePilot makes one only
after a `ready` answer, cached for 30 s). It answers from what holds NOW:

1. **`no_user_manager`**: the person's systemd user manager as the
   session's start found it, and as it is now: `systemd-run`, `busctl`,
   `systemctl`, `/usr/lib/systemd/systemd` and `dbus-daemon` there, its
   private socket (`$XDG_RUNTIME_DIR/systemd/private`) there, and the
   memory and pids controllers given to it (`user@<uid>.service`'s
   `cgroup.controllers`), or a space's limits would not hold.
2. **`guard`**: the space guard installed and its probe answered, and
   polkit refusing a space's process (the package's rule is read).
3. **`session`**, the latch: once a space's session failed to start (its
   user manager could not start, or ended), until a session start
   succeeds (a manager that runs and whose bus is there). What failed is
   refused: a compositor whose launches run in scopes never starts one in
   the person's login session, and one that lost its user manager answers
   `unavailable`.

There is no `driver` block: on every driver a space's apps start, on the
GPU where the driver is proven and in software elsewhere (below). Each
answer that differs from the one before is logged with its cause
(`agent spaces are not offered (guard): …`); the device's doctor shows
the row Agent space (Ready, or why not) and `exponential doctor` names
the cause.

## Agent spaces on the GPU (#EDE-162)

A space's apps draw on a GPU, as they would on the person's desk: Vulkan,
OpenGL/EGL and a browser's WebGL in hardware, instead of llvmpipe, lavapipe
and SwiftShader.

- **Which GPU** (`[space_gpu] device`, read at the compositor's start):
  `auto` takes, of the cards with a GPU driver and a render node:

  | The machine | Spaces render on |
  |---|---|
  | one GPU (a desktop with a dGPU and no iGPU, most laptops) | that GPU |
  | two GPUs, no battery, the displays on one (a desktop: monitors on the dGPU, an idle iGPU) | the one the displays are not on: the person's game keeps its GPU to itself |
  | two GPUs and a battery (a laptop: the iGPU drives the panel, the dGPU sleeps) | the one driving the displays: the dGPU stays asleep |

  `auto` takes only drivers proven for spaces (`nvidia`, `i915`, `xe`); on
  any other (AMD, nouveau, a VM's GPU) spaces render in software and the log
  says why. Naming a render node opts in on any driver
  (`device = "/dev/dri/renderD128"`); proving AMD and Mesa as the main GPU
  is #EDE-164. `software` keeps spaces in software; so do safe mode, no GPU,
  a renderer that turns out to be llvmpipe, and NVIDIA without explicit sync.
- **What it is.** One renderer for all spaces, opened at the start on a
  helper thread: its own EGL display on the render node
  (EGL_EXT_platform_device: no display, no DRM master), its own context at
  low priority where the driver has one; the person's renderer is not
  touched. A space's clients get the spaces' own linux-dmabuf (that GPU as
  the main device, explicit modifiers only) and drm-syncobj; the person's
  clients, a space in software and a space's Xwayland never see them (X11
  GL apps in a space draw in software; glamor per space is #EDE-163). Its
  programs see exactly that GPU's Vulkan driver and lavapipe
  (`VK_ICD_FILENAMES`, as in software; lavapipe alone where that driver is
  not installed): a Vulkan app takes the space's GPU, never the person's.
  A browser in it keeps hardware ANGLE: of F3's software switches it gets
  only `--enable-unsafe-swiftshader`, ANGLE's own fallback.
- **The person's frames come first.** A space's tick builds no renderer
  elements and imports nothing. A capture is drawn after the person's frames
  of that turn, read back asynchronously (a fence the loop waits on, never
  the thread), at most once per space per 50 ms and 100 ms of the thread per
  second for all spaces. A space's app presents at most once per tick of the
  space's clock (60 Hz in use, 10 Hz unwatched): an app that renders faster
  (vsync off) has its extra frames held and dropped. GPU work an app never
  presents (compute) is not bounded by that: the GPU choice above is what
  keeps it off the person's GPU. `EXP_FRAME_STATS=1` prints the spaces' cost
  on the person's thread every 5 s; `EDE_HEADLESS_GLES=/dev/dri/renderDN` (a
  measuring rig) draws a headless compositor's own output on a GPU, as udev
  draws a monitor, to measure the person's frame times beside spaces.
- **The safety net: an app whose GPU path fails falls back to software.** In
  a space on the GPU, any of these latches the space back to software:
  - a launch that ends non-zero or by a signal within 10 s without any
    client of it having drawn (vkcube's missing present queue, ANV's
    swapchain abort, a Wine or GLFW X11 Vulkan app: X11 apps draw through
    the space's Xwayland and always count as not having drawn), or that
    starts dumping a core within those 10 s without having drawn (counted
    at once: writing the core can keep a crashed app alive for half a
    minute);
  - a GPU buffer the space's GPU cannot import, or more than 32 a second
    from one client (refused; a window that cannot be drawn shows a grey
    placeholder);
  - a GPU buffer whose fence never fires within 2 s;
  - a lost GPU context (every GPU space).

  Its later clients and launches then draw in software, the running ones
  stay as they are. The agent reads "X ended at start on the GPU (...);
  this space now draws in software: start it again" with its next
  screenshot. After three such spaces in one run, new spaces start in
  software.
- **What the net does not catch:** an app that draws and then shows black or
  freezes, a blank canvas inside a window that draws. The hand proof per
  driver is the only check for those, which is why `auto` takes proven
  drivers only.

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
command = ["swaylock", "-f", "-c", "000000"]   # any ext-session-lock client; empty (the default) = the sealed lock, `exponential-sealed lock`
on_suspend = true     # lock before suspend and on `loginctl lock-session` (udev)

[idle]
lock_after = 600      # seconds without input; 0 = never

[protected]
app_ids = ["org.example.Bank"]   # on top of the built-in list

[spaces]              # what agent spaces may take of this machine (#EDE-138); the defaults:
# max = 8             # spaces at once; unset = one per GiB of memory_max (rounded), at most 32
launches = 32         # programs one space may have launched at once
cpu_quota = 50        # percent of all cores, for all spaces together
memory_max = 50       # percent of the memory, for all spaces together
space_memory_max = 30 # percent of the memory for one space (at least 2 GiB, never more than memory_max)
tasks_max = 4096      # processes and threads of one space

[space_gpu]           # the GPU agent spaces render on (#EDE-162)
device = "auto"       # or "software", or a render node: "/dev/dri/renderD128" (taken whatever its driver)

[theme]              # the bridge (#EDE-19), read by exponential-theme, not the compositor
bridge = true        # false: exponential-theme sync removes what it wrote
icons = "Papirus-Dark"   # gtk, cursor, cursor_size, font, monospace, font_size likewise; unset = the first installed preference

[diagnostics]        # Settings › About › Send diagnostics (#LOGS-2)
send = true          # false (or EXP_DIAGNOSTICS=0): no crash, performance or log reports
```

## Diagnostics (#LOGS-2)

`crates/ede-observability`: the Sentry SDK in every long-running binary (#ERR-29: the compositor, `exponential-session`, `exponential-services serve`, `exponential-shell`, `exponential-sealed`, the portal and `exponential-ui supervise`, each with its own `component` tag; panics, `error!` events, `warn!`/`info!` breadcrumbs and logs, transactions, native crashes; the shell and the sealed surfaces, where the person's content passes through, send only WHERE a line was logged, never its text: `Trail::Places`),
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
`swaylock` today), or with no `[lock] command` the sealed lock
(`exponential-sealed lock`, above). From the lock request on, every output draws only that
client's lock surface (black until it draws), only it gets keys and clicks,
and bindings are off except Ctrl+Alt+F1…F12. If the lock client crashes the
session stays locked (black) until a new lock client takes over; a second
client can neither lock over it nor unlock it. Super+L, `[idle] lock_after`
(held off by idle inhibitors such as a playing video), the `lock` op,
`loginctl lock-session` and suspend run `[lock] command`, or the sealed lock
when it is empty; before suspend a logind delay inhibitor waits up to 2 s for
the lock to show. `ext-idle-notify` is there for
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
panel.json, rolled back`), unless another process undid or replaced it
meanwhile (`exponential-ui undo`, Version history): nothing is rolled back
and what replaced it is an edit of its own. A rollback puts back what the
shell last drew (after two quick edits, never the first one, which did not
draw either) and touches that document alone: another document's edit in
the same moment stays an edit of its own, and a broken text the file holds
meanwhile is kept beside it as `.panel.json.refused`. The shell dying counts as a crash, it restarts
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
