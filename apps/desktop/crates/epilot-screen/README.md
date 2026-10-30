# epilot-screen

ePilot's screen memory: the last 10 minutes of the screen as deduplicated
frames, and ONE MCP tool, `get_screenshots`, that hands an agent the frames of a
time range ("what was I looking at when I said that"). Read-only, local, on a
loopback MCP endpoint of its own with the [`epilot`](../epilot) topology (one
listener per process, a per-run bearer token, plain JSON).

Its only path dependency is `epilot` (the window list and display order, so
frame numbers match `epilot::displays()`). gpui-free, talks to no server; the
host hands it a data directory.

## Consuming

`publish = false`; a git dependency on the `epilot-v<version>` tag, as for
`epilot` (see its README). On Linux turn on the `portal` feature for the xdg
ScreenCast portal + PipeWire capturer (libpipewire is a LOAD-time dependency, so
a headless daemon leaves it off and reads the X root instead):

```toml
epilot-screen = { git = "https://github.com/Niach/exponential", tag = "epilot-v0.14.83", features = ["portal"] }
```

The spec is `packages/domain-contract/fixtures/screen-memory.json`, compiled in
with `include_str!` (`src/fixture.rs`): every number, the copy, the tool's
schema and description, the denylist, and the cases the tests replay.

## Plan → act → observe

```rust
// Plan: can this machine ever capture? (needs epilot's window list)
if !epilot_screen::available() { return; }
epilot_screen::set_status_observer(|status| tray.show(status)); // Off / Active / Paused

// Act: capture into <data dir>/screen + <data dir>/screen.sqlite.
let ticket = epilot_screen::ticket();          // take when the start is ASKED for
std::thread::spawn(move || epilot_screen::start_with(data_dir, ticket));
epilot_screen::set_hotkey_paused(true);        // a host pause key (also the denylist pauses)

// Observe: an agent run reads the buffer through its grant.
let grant = epilot_screen::grant("run-1")?;    // Err while off
// MCP config: { "screen": { "type": "http", "url": grant.url,
//               "headers": { "Authorization": "Bearer ${EXP_SCREEN_TOKEN}" } } }
epilot_screen::revoke("run-1");

epilot_screen::stop(/* forget */ true);        // off: delete every frame + row
epilot_screen::forget(&data_dir);              // sign-out before any start
epilot_screen::leftover(&data_dir);            // a buffer left on disk at start-up?
```

A stop cancels every start asked for before it (`CANCELLED`), also one waiting
on the Linux portal's dialog. `examples/live.rs` runs capture + store + endpoint
end to end; `examples/capture_mac.rs` benches the macOS capturer.

### `get_screenshots {from?, to?, filter?}`

RFC 3339 `from`/`to` (default: the last 60 s), `filter` = a case-insensitive
substring of the app or window title. Returns up to 4 frames of displays that
are work displays NOW (evenly picked across the range), each a `{at} · {app} —
{title}` line plus the stored WebP, size-capped per image and in total.

## What it stores

- **Capture**: every work display once a second plus the focused window's display
  at once on a focus or title change (macOS ScreenCaptureKit with the host app
  excluded, Linux the ScreenCast portal + PipeWire or the X11 root, Windows
  Graphics Capture).
- **Dedupe**: a 64-bit perceptual hash (64×64 luma, 8×8 blocks); a frame with the
  same app + title and a Hamming distance ≤ 5 to the display's last kept frame is
  dropped.
- **Store**: cropped to the focused window, long edge ≤ 1568, lossy WebP in
  `<data dir>/screen/<UTC yyyyMMdd-HH>/<at ms>-<display>.webp`, one row each in
  `<data dir>/screen.sqlite` (`screen_frames`); encoded on one low-priority worker
  held under 1 % of a core.
- **Retention**: rows and files older than 10 minutes go every second (paused or
  not), empty hour directories too; at start the same sweep runs and files
  without a row are deleted. The tool never returns an older frame. A frame the
  tool returned becomes part of that agent's transcript, outside this crate.
- **Denylist** (`denylist.apps`, `denylist.titles`: password managers, private
  browsing, banking): capture PAUSES while the focused window matches; any other
  matching window on a captured display is filled black before the hash and in
  the stored picture. No window list = no capture (fail closed).

## Permissions per OS

| OS | Needs | Without it |
| --- | --- | --- |
| macOS | **Screen Recording** (ScreenCaptureKit) | `start` answers `Err` |
| Linux X11 | `portal` on: the ScreenCast portal asks once (restore token kept in the data dir); off: a `DISPLAY` | `start` answers `Err` |
| Linux Wayland / no display | not available (no window list, so the denylist cannot hold) | `available()` is `false` |
| Windows | nothing up front | — |

## Exponential couplings

None at runtime. Exponential-named defaults: the `screen` server name, the
`EXP_SCREEN_TOKEN` constant (only the host writes it into an MCP config), the
macOS dispatch queue label `at.exponential.screen-memory` and the PipeWire node
names `exponential-screen-<n>` (diagnostic only), and the fixture path above.
In Exponential the switch, the private-conversation rule and who gets a grant
live in the host (`ui` tray, `coding` launcher), not here.
