# epilot

Computer use as a library: an agent drives this machine's **real** pointer and
keyboard through the OS input APIs (XTest on X11, CGEvent on macOS, SendInput on
Windows) and sees the screen as screenshots fitted to the model's frame. While it
works the person's own input is locked; **Esc** aborts. Foreground only: no second
pointer, no background mode, no accessibility tree, pixels only.

The host process serves ONE loopback MCP endpoint (streamable HTTP, plain JSON
answers, a per-run bearer token). Any MCP-speaking agent CLI reaches it with a
config entry; the crate is gpui-free, tokio-free and talks to no server.

Exponential links it from the IDE and the `exponential` daemon (through
`coding::computer`); spezl links it as a git dependency.

## Consuming

`publish = false`: consume it as a git dependency on a release tag. Tags are
`epilot-v<version>`, `<version>` being the crate's `version` (today the desktop
workspace's). `epilot-screen` ships under the same tag. No `epilot-v*` tag has
been cut yet; until the first one, pin a `rev` instead.

```toml
[dependencies]
epilot = { git = "https://github.com/Niach/exponential", tag = "epilot-v0.14.84" }
epilot-screen = { git = "https://github.com/Niach/exponential", tag = "epilot-v0.14.84" }
```

The rules are compiled in from `packages/domain-contract/fixtures/` with
`include_str!` (a git checkout carries them; a crates.io package would not):

| Fixture | What it decides |
| --- | --- |
| `work-screen.json` | which displays agents may drive (the picked one, or Auto = every display without a fullscreen app), the `wrong_display` refusals, where an overlay may open |

## Plan → act → observe

```rust
// Plan: read readiness without prompting, or ask for every OS permission now.
match epilot::prepare() {                      // blocks on a dialog; see prepare_in_background
    epilot::Readiness::Ready => {}
    other => return eprintln!("{other:?}"),
}
epilot::set_work_display(None);                // None = Auto; Some(id) = a Display::id

// Act: admit a run; the agent gets the URL + its token.
let grant = epilot::grant("run-1", "EXP-12")?; // label = what the banner names
epilot::on_abort("run-1", Arc::new(|| cancel_the_turn()));
// MCP config: { "computer": { "type": "http", "url": grant.url,
//               "headers": { "Authorization": "Bearer ${EXP_COMPUTER_TOKEN}" } } }

// Observe: the lock engages (Some(label)) and releases (None).
epilot::set_presence_observer(|label| show_or_hide_banner(label));

epilot::turn_started("run-1");                 // the person's next message: an abort is answered
epilot::revoke("run-1");                       // the run ended: token dead, lock released if last
epilot::shutdown();                            // the switch went off: every grant ends
```

`examples/host.rs` is this as a runnable host (`cargo run -p epilot --example host`);
`examples/serve.rs` calls every tool by hand, `examples/work_screen.rs` prints the
display picture.

### The public API

| Item | Role |
| --- | --- |
| `readiness()` / `permissions()` | the state now, never prompting (a doctor line) |
| `prepare()` / `prepare_in_background()` | ask for the permissions and prove the backend opens |
| `grant(session_id, label) -> Grant` | start the endpoint once, mint the run's token (an earlier one of the same run dies) |
| `revoke`, `turn_started`, `on_abort`, `shutdown`, `running()` | the run's lifecycle |
| `set_presence_observer(Fn(Option<String>))` | the host's banner hook; called at once with the current state |
| `SERVER_NAME` (`"computer"`), `TOKEN_ENV` (`"EXP_COMPUTER_TOKEN"`) | the MCP config key and the env var the config names; a host may use its own env var name, the crate never reads it |
| `displays()`, `pixel_size`, `screen_layout`, `screen_state`, `banner_display`, `set_work_display`, `picked_display_id` | the display picture (work screen) |
| `screen_view()`, `focused_window()`, `visible::*` | the window list (focused window, top window per display, a raw screenshot), shared with `epilot-screen` |
| `work_screen::*` | the pure work-screen rules (gate, overlay display) |

### The tools (`src/tools.rs`)

Every coordinate is in the last screenshot's pixels (fitted to at most 1280×800).

| Tool | Does |
| --- | --- |
| `screenshot {display?}` | the current display as a PNG; `display` switches the run to another work display |
| `list_windows` | top-level windows on the work screen, top first, plus the work displays |
| `activate_window {id}` | bring a window to the front with the keyboard |
| `cursor_position` | where the pointer is |
| `mouse_move`, `left_click`, `right_click`, `middle_click`, `double_click {x, y}` | pointer |
| `drag {from_x, from_y, to_x, to_y}`, `scroll {x, y, direction, amount?}` | pointer |
| `type {text}` (≤ 4000 chars, newline = Return), `key {combo}` (`"ctrl+s"`, `"cmd+space"`) | keyboard |
| `wait {seconds ≤ 10}` | let the screen settle, then a screenshot |

A call on a display outside the work screen answers a refusal the agent reads
(`work_screen::{NO_WORK_SCREEN, WINDOW_ELSEWHERE, …}`), never a JSON-RPC error.

### The lock and Esc

The person's keyboard and pointer are taken on a run's first call (XInput2 grabs
of the person's own devices, a CGEventTap, low-level hooks) and stay taken while
runs keep calling, so a keystroke between the agent's screenshot and its click
cannot land. It releases when every run that used it ended, after 30 s without a
call, or on **Esc**: then every run that used it answers `aborted` until its next
`turn_started`, and each run's `on_abort` hook fires (cancel its turn). The
agent's own injected events are tagged and pass the lock.

## Permissions per OS

| OS | Needs | Without it |
| --- | --- | --- |
| macOS | **Accessibility** (input + the event tap lock) and **Screen Recording** (screenshots, window titles); `prepare()` triggers both prompts, `permissions()` reads them | `Readiness::MissingPermission("… is not granted.")` |
| Linux X11 | a `DISPLAY` (pure-Rust x11rb, nothing at load time: a headless daemon links it too) | `Unsupported("No display session (DISPLAY is not set).")` |
| Linux Wayland | not supported yet (sign in to an X11 "on Xorg" session) | `Unsupported(…)` |
| Windows | nothing up front (SendInput, GDI capture, per-monitor DPI awareness set on open) | — |

## Exponential couplings

None at runtime: no server, no settings file, no env var read. What is
Exponential-named is a default a host can live with: the `computer` server name
and the `EXP_COMPUTER_TOKEN` constant (only the host writes it into an MCP
config), and the fixture path above.
