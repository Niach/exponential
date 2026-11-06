# Exponential Desktop (eDE internally)

The Exponential Desktop Environment (#NEW-10): a Wayland compositor plus a
shell drawn in genUI. This is its own Cargo workspace; it depends on the shared
libraries, never the other way round.

| Crate | Role |
|---|---|
| `exponential-session` | What the login screen starts: the session environment (`XDG_CURRENT_DESKTOP=Exponential`, Wayland hints for Firefox/Qt/Electron), `exponential-compositor` on the seat, then `exponential-session.target` (XDG autostart via systemd's generator, polkit agent, gnome-keyring). Restarts a crashed compositor; three quick crashes in a row (also across logins) bring it back in safe mode. |
| `ede-protocol` | The control plane (#NEW-104): wire types, framing and a blocking `Client` for the compositor's socket, on Core's `Capability`/`Grant`/`Failure`/`Envelope`. Computer Use, eos-agent and the shell link it. |
| `exponential-ui` | The shell's genUI on disk (#EDE-9): documents in `~/.config/exponential-desktop/ui` (factory copies in `/usr/share/exponential-desktop/ui`), validated before they apply, hot reloaded, the last good version kept, every edit a git commit, undo = revert. The shell and ede-tools link it; `exponential-ui` is its CLI. |
| `exponential-ctl` | The command line over `ede-protocol` (#EDE-7): `windows`, `outputs`, `capture --png` (the shots capture and the agent harness in the VM), `key`, `pointer`. |
| `exponential-compositor` | smithay compositor: floating windows with magnetic edges and edge tiling, fullscreen, xdg-shell, wlr-layer-shell, Xwayland (X11 apps and games), linux-dmabuf, drm-syncobj explicit sync (udev), ext-session-lock, idle notify/inhibit, input methods (input-method-v2, text-input-v3, virtual keyboard), protected surfaces, damage-tracked rendering, `~/.config/exponential-desktop/compositor.toml`. Backends: `winit` (nested, development), `udev` (the real session: DRM/KMS, libinput, hotplug; feature `udev`) and `headless` (CI). |

## The shell (`shell/`, a second workspace)

`apps/ede/shell` is its own Cargo workspace (#EDE-10): `exponential-shell` draws the
shell from the genUI documents `exponential-ui` serves (below) with gpui + eui-gpui
at the SDK's pins (the painter README's "gpui revisions", gated by
`check-gpui-revs.ts`), Wayland backend only. The compositor workspace never
links gpui, and the shell is a separate process under the supervisor, so a
shell crash never takes the session down. Every document with a layer
`surface` header is a layer-shell surface (`WindowKind::LayerShell`); a
popup document opens from its anchor component in its parent as an
`xdg_popup` of that surface (so a menu can leave the panel; eui-gpui paints
its own overlays inside their window). A change on disk redraws the surface
and the shell prints the document's frame line, which the supervisor reads.
Data every document may bind: `/time` (local), `/windows` (the open
windows over ede-protocol), `/apps` + `/query` (the launcher: the desktop
entries of the XDG data directories, filtered as the person types),
`/switcher` (the open windows most recent first with live thumbnails),
`/osd` (a level). Host functions, each only from a document whose
`surface.functions` lists it (#EDE-8's Tier C rule, checked by `exponential-ui` and
again by the shell): `ede.launch {command}`, `ede.focus {window}`,
`ede.close {window}`, `ede.lock`, `ede.logout`, `ede.power {action:
suspend|reboot|poweroff}`, `ede.select {window}` (the switcher).
Icons resolve by concept from `packages/icons` (the IDE's Lucide files and
fonts are embedded); app icons come from the installed icon themes as
`file:` pictures. Factory documents live in `apps/ede/ui/*.json`:
`panel.json` (top, 40 px exclusive zone, the launcher button, the taskbar,
the clock), `menu.json` (the panel's popup: search, apps, the power row),
`switcher.json` and `osd.json` (`onDemand` overlays the shell opens on a
hotkey and closes again), `notifications.json` (the toasts, top-right, on
demand), `notification-center.json` (the bell's popup) and `tray-menu.json`
(a tray item's menu as the tray's popup). A click outside an open popup, or
Escape in it, closes it.

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
Host functions: `ede.notify_close {id}`, `ede.notify_action {id, key}`,
`ede.notify_clear`, `ede.tray {service}`, `ede.tray_menu {service, id}`.

### Hotkeys (#EDE-12)

The compositor keeps these keys and tells the shell over ede-protocol
(`hotkey` events; the shell's layer surfaces never hold the keyboard while
an app has it): **Alt+Tab** opens the switcher and cycles it (Shift
reverses), releasing Alt focuses the selected window, Escape cancels, a
click on a tile picks it; **Super** alone toggles the menu (its search box
takes the keyboard); **Print** saves `~/Pictures/Screenshots/Screenshot
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
directory's `panel.json` and the panel redraws within the watcher's
debounce; break it and `exponential-ui check` says why while the last good version
stays on screen; `git -C <dir> log` shows `Edit panel.json` commits after
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

## Settings (#EDE-14)

`settings.json`, a shell document like the rest (editable, in the
history), not an eApps package: one on-demand surface in the middle of the
screen (the menu's Settings row opens it with `ede.open_document`, its
close button `ede.close_document`) with a page per domain: Displays
(every output's modes and scale, `ede.output` applies live and writes
`[[outputs]]`), Network (Wi-Fi switch, connections, networks in reach),
Sound (volumes, mute, output devices), Bluetooth, Power (battery, profile,
suspend/restart/shut down), Input (keyboard layout and repeat,
`ede.config` writes `[keyboard]` and reloads), Theme (the bridge's switch
and what it resolved, #EDE-19), Apps & updates (apt, Software), Users, Printers (CUPS),
About. Pages bind `/services/<domain>` (#EDE-13), `/outputs`, `/config`
and `/user`; a control is two-way bound and calls `ede.service {domain,
action, args}` with the new value. Users and printers management open the
native tools until there is more to show.

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
(NetworkManager: state, devices, Wi-Fi access points; connect, disconnect,
Wi-Fi on/off), `sound` (PipeWire through `wpctl`: default sink and source,
volumes, mutes; set, mute, default), `bluetooth` (BlueZ: adapters,
devices; power, connect, disconnect, discover), `power` (UPower battery,
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
  description names (`ui` for volume, `system` for the rest), a refusal
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
headless outputs resize. Every surface hears its output's preferred scale
(fractional through wp-fractional-scale-v1; integer clients keep
`wl_surface.set_buffer_scale`). Direct scanout: a fullscreen window alone
on an output is the frame's only element, and smithay's DRM compositor
puts it straight on the primary plane when its buffer fits (the default
frame flags allow it); the udev backend sends such windows a dmabuf
feedback with a scanout tranche of the plane's formats so games and video
allocate scanout-able buffers. Hybrid GPUs: one render GPU (the primary),
outputs on the other card render through it (smithay's multi-GPU
renderer); a per-output render device is not done.

## Doctor

```bash
cargo run -- doctor
```

Checks what the real session needs, from inside any desktop (no seat is
taken): the GPU render nodes; the KMS driver of every `/dev/dri/card*` and
its connected displays (the virtual ones say what to expect: `vmwgfx` is
VMware's and VirtualBox's VMSVGA, software rendering unless the VM has 3D);
the seat (logind's `seat0` with a graphical device, or seatd); what EGL and
GLES render with on the render node, the way the udev backend opens it
(`llvmpipe` = software rendering, fine in a VM; a udev build only, a
winit-only build says so); the keyboard layout the session starts with
(`/etc/default/keyboard`, else xkb's `us`); Xwayland (the binary the compositor will run
and its version: 24.1+ has explicit sync; older versions flicker with X11
games on NVIDIA); and on NVIDIA that `nvidia-drm.modeset` is on (the
parameter, the kernel command line, or the driver default from 560).

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
(`renderD128`). `exponential-compositor recovery --reason … --png out.png --size
1280x800` draws the message into a file for a look.

## ede-protocol

`$XDG_RUNTIME_DIR/exponential-desktop/<wayland-N>.sock` (directory 0700): frames of a
little-endian `u32` length plus JSON (`{"id": 1, "op": "list_windows"}`),
the pixels of a capture as the frame after its reply. `hello` with a token
first; each op needs the Core capability it names:

| Op | Capability |
|---|---|
| `list_windows`, `list_outputs`, `subscribe` (events: `window.opened/closed/focused`, `session.locked/unlocked`, `config.reloaded`) | any grant |
| `focus_window`, `close_window`, `move_window` | `ui` |
| `capture` (an output: protected windows black, locked = black; a window: protected ones refused) | `capture` |
| `input` (key, pointer motion, button; refused on protected surfaces and the lock) | `input:person` |
| `reload_config`, `lock`, `mint_grant`, `revoke_grant` | `system` |

The compositor writes the root token (every capability) to
`$XDG_RUNTIME_DIR/exponential-desktop/<wayland-N>.token` (0600) for the shell; `mint_grant`
hands narrower, expiring grants to agents (never more than the minting grant
holds, never outliving it). Any process of the same user can read the token
file, so the boundary is between the shell and the agents it mints for (an
agent sandbox does not see `$XDG_RUNTIME_DIR/exponential-desktop`). Agent spaces and window
lending come with their own issues; captures are drawn in software for now.

```rust
let mut ede = ede_protocol::Client::connect_env()?;   // $WAYLAND_DISPLAY + the token file (or $EDE_TOKEN)
for w in ede.windows()? { println!("{} {:?}", w.id, w.title); }
```

## Apps (#EDE-20)

`APPS.md`: Mint's default apps against what eDE keeps, replaces with the
shell and settings, or rewrites as eApps packages once the `eapps` crate
exists (#EXP-1420); Calculator, System Monitor and Software are parked
there with what each needs first.

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
before the shell, the Theme page's switch runs it after `ede.config`
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
```

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
           "children": [{"id": "clock", "component": "Text", "props": {"text": {"path": "/time"}}}]},
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
