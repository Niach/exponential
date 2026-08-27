// Clean reimplementation from the VT spec + rio-vt (MIT). NOT derived from Zed's GPL terminal crates.
//! Headless integration tests for the gpui-free terminal core (masterplan-v3
//! §6.2 / §11.4 Phase-4 gate): real PTY, real children (`bash`, `sh`, `vim`),
//! grid-level assertions — no window, no gpui.

use std::time::{Duration, Instant};
use terminal::rio_vt::config::colors::{AnsiColor, NamedColor};
use terminal::{SpawnSpec, Terminal, TermMode};

// Generous: these tests spawn real children (`bash`, `vim`) and a full
// `cargo test --team` runs them alongside every other binary — under
// that load a 15s deadline has flaked while standalone runs finish in <1s.
const LONG: Duration = Duration::from_secs(60);

fn bash_spec() -> SpawnSpec {
    SpawnSpec::new("bash").args(["--noprofile", "--norc"]).env("PS1", "$ ")
}

/// Pump events (writing §6.6 replies back to the PTY) until `pred` holds or
/// the timeout elapses. Waits on the wake channel between pumps — the same
/// discipline the gpui foreground drain will use.
fn pump_until(term: &mut Terminal, timeout: Duration, pred: impl Fn(&Terminal) -> bool) -> bool {
    let wake = term.wake_rx();
    let deadline = Instant::now() + timeout;
    loop {
        term.pump();
        if pred(term) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        let _ = wake.recv_timeout(Duration::from_millis(50));
    }
}

/// Kill the child and pump until its exit is reaped, or fail with diagnostics.
///
/// EXP-527: `Terminal::kill` is portable-pty's unix killer — a single SIGHUP,
/// which a shell may defer (readline mid-redisplay) or which a loaded CI
/// runner can leave unacted-on past any reasonable deadline; the plain
/// `kill(); pump_until(exit)` form flaked at 60s on a runner under full
/// workspace-test load. So: keep re-sending the kill (idempotent) while
/// pumping, and after `escalate_after` fall back to an unignorable SIGKILL —
/// the wait thread is parked in `child.wait()`, so once the process dies the
/// reap is immediate. A failure past that names the child's actual process
/// state instead of just "kill never reaped".
fn kill_until_reaped(term: &mut Terminal, escalate_after: Duration) -> Result<(), String> {
    let pid = term.process_id();
    let wake = term.wake_rx();
    let deadline = Instant::now() + LONG;
    let escalate_at = Instant::now() + escalate_after;
    let mut escalated = false;
    term.kill();
    loop {
        term.pump();
        if term.exit().is_some() {
            return Ok(());
        }
        let now = Instant::now();
        if now >= deadline {
            break;
        }
        if !escalated && now >= escalate_at {
            escalated = true;
            if let Some(pid) = pid {
                let _ = std::process::Command::new("kill")
                    .args(["-KILL", &pid.to_string()])
                    .status();
            }
        } else if !escalated {
            term.kill(); // re-send SIGHUP in case the first delivery was lost
        }
        let _ = wake.recv_timeout(Duration::from_millis(50));
    }
    let state = pid.and_then(|pid| {
        std::process::Command::new("ps")
            .args(["-o", "state=", "-p", &pid.to_string()])
            .output()
            .ok()
            .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
    });
    Err(format!(
        "kill never reaped within {LONG:?} (pid {pid:?}, ps state {state:?}, \
         escalated to SIGKILL: {escalated})"
    ))
}

fn grid_contains(term: &Terminal, needle: &str) -> bool {
    term.screen_lines().iter().any(|line| line.contains(needle))
}

/// True when `needle` appears on a line that is NOT the echoed input (the
/// echoed command still contains the quote-split form).
fn output_line_contains(term: &Terminal, needle: &str) -> bool {
    term.screen_lines()
        .iter()
        .any(|line| line.contains(needle) && !line.contains("echo"))
}

fn dump(term: &Terminal) -> String {
    term.screen_lines().join("\n")
}

#[test]
fn shell_runs_commands_and_grid_shows_output() {
    let mut term = Terminal::spawn(&bash_spec(), 80, 24).expect("spawn bash");
    // Quote-split marker: the echoed input line shows `hello-'t'erminal`,
    // only the real output shows the joined form.
    term.write(b"echo hello-'t'erminal\n");
    assert!(
        pump_until(&mut term, LONG, |t| output_line_contains(t, "hello-terminal")),
        "grid never showed the echo output:\n{}",
        dump(&term)
    );
    // build_command's env applied end-to-end (§6.12): TERM reaches the child.
    term.write(b"echo term-is=$TERM\n");
    assert!(
        pump_until(&mut term, LONG, |t| grid_contains(t, "term-is=xterm-256color")),
        "TERM not xterm-256color:\n{}",
        dump(&term)
    );
    term.write(b"exit\n");
    assert!(pump_until(&mut term, LONG, |t| t.exit().is_some()), "bash never exited");
    assert!(term.exit().expect("exit captured").success);
}

#[test]
fn resize_mid_run_delivers_sigwinch_and_reflows() {
    let mut term = Terminal::spawn(&bash_spec(), 80, 24).expect("spawn bash");
    term.write(b"stty size\n");
    assert!(
        pump_until(&mut term, LONG, |t| grid_contains(t, "24 80")),
        "initial winsize not 24x80:\n{}",
        dump(&term)
    );

    // Grow. TIOCSWINSZ → SIGWINCH; the child's tty must report the new size.
    term.resize(120, 30).expect("resize grow");
    assert_eq!(term.size(), (120, 30));
    term.write(b"stty size\n");
    assert!(
        pump_until(&mut term, LONG, |t| grid_contains(t, "30 120")),
        "winsize after grow not 30x120:\n{}",
        dump(&term)
    );

    // Shrink — the nastier direction (reflow/truncate) — no panic.
    term.resize(40, 10).expect("resize shrink");
    term.write(b"stty size\n");
    assert!(
        pump_until(&mut term, LONG, |t| grid_contains(t, "10 40")),
        "winsize after shrink not 10x40:\n{}",
        dump(&term)
    );
    assert_eq!(term.screen_lines().len(), 10);

    // §6.9/§6.10 guards: zero-size and no-op resizes are ignored, not passed
    // to the emulator (which rejects zero grids) or the child (SIGWINCH storm).
    term.resize(0, 10).expect("zero cols ignored");
    term.resize(40, 10).expect("no-op ignored");
    assert_eq!(term.size(), (40, 10));
}

#[test]
fn cjk_and_emoji_occupy_wide_cells_without_smear() {
    let mut term = Terminal::spawn(&bash_spec(), 80, 24).expect("spawn bash");
    // "你好 🌍 ok" — CJK + emoji, all double-width.
    term.write("echo 你好 🌍 ok\n".as_bytes());
    assert!(
        pump_until(&mut term, LONG, |t| grid_contains(t, "你好 🌍 ok")),
        "wide sample never rendered:\n{}",
        dump(&term)
    );

    // Grid-level wide-cell contract (§6.9): each wide glyph owns its cell
    // as a wide square and the trailing cell is a spacer.
    let handle = term.term();
    let guard = handle.lock();
    let cells: Vec<(i32, usize, char, bool, bool)> = guard
        .grid
        .display_iter()
        .map(|cell| {
            let square = *cell.square;
            (cell.pos.row.0, cell.pos.col.0, square.c(), square.is_wide(), square.is_spacer())
        })
        .collect();
    drop(guard);

    for wide in ['你', '好', '🌍'] {
        let (line, column, _, is_wide, _) = *cells
            .iter()
            .find(|(_, _, c, _, _)| *c == wide)
            .unwrap_or_else(|| panic!("{wide} not in grid"));
        assert!(is_wide, "{wide} not flagged wide");
        let (.., is_spacer) = *cells
            .iter()
            .find(|(l, col, _, _, _)| *l == line && *col == column + 1)
            .unwrap_or_else(|| panic!("no cell after {wide}"));
        assert!(is_spacer, "cell after {wide} is not a wide spacer");
    }

    // Spacer-skipped reconstruction reads back exactly (no smear/doubling).
    assert!(
        term.screen_lines().iter().any(|line| line.contains("你好 🌍 ok")),
        "reconstructed line smeared:\n{}",
        dump(&term)
    );
}

#[test]
fn dsr_query_reply_reaches_the_child() {
    // The claude-hang guard (§6.6): the child probes with DSR and BLOCKS on
    // the reply; only pump()'s PtyWrite forwarding un-blocks it.
    let mut term = Terminal::spawn(&bash_spec(), 80, 24).expect("spawn bash");
    term.write(b"printf '\\033[6n'; IFS=R read -rs -d R pos; echo \"DSR-OK:${pos#*[}\"\n");
    let ok = pump_until(&mut term, LONG, |t| {
        t.screen_lines().iter().any(|line| {
            line.split("DSR-OK:")
                .nth(1)
                // Echoed input shows `DSR-OK:${pos#*[}` — a real reply is
                // `row;col`, so require a leading digit.
                .is_some_and(|rest| rest.chars().next().is_some_and(|c| c.is_ascii_digit()))
        })
    });
    assert!(ok, "DSR reply never came back (PtyWrite path broken):\n{}", dump(&term));
}

#[test]
fn osc_title_is_tracked() {
    let mut term = Terminal::spawn(&bash_spec(), 80, 24).expect("spawn bash");
    term.write(b"printf '\\033]0;headless-title\\007'\n");
    assert!(
        pump_until(&mut term, LONG, |t| t.title() == Some("headless-title")),
        "title never tracked; title={:?}",
        term.title()
    );
}

#[test]
fn child_exit_code_is_captured() {
    let mut term =
        Terminal::spawn(&SpawnSpec::new("sh").args(["-c", "exit 7"]), 80, 24).expect("spawn sh");
    assert!(pump_until(&mut term, LONG, |t| t.exit().is_some()), "exit never captured");
    let exit = term.exit().expect("exit state");
    assert_eq!(exit.code, 7);
    assert!(!exit.success);
    assert!(!term.is_running());
}

#[test]
fn kill_ends_a_running_child() {
    let mut term = Terminal::spawn(&bash_spec(), 80, 24).expect("spawn bash");
    term.write(b"echo ready-'m'arker\n");
    assert!(
        pump_until(&mut term, LONG, |t| output_line_contains(t, "ready-marker")),
        "shell never became ready:\n{}",
        dump(&term)
    );
    assert!(term.is_running());
    kill_until_reaped(&mut term, Duration::from_secs(5)).unwrap();
    let exit = term.exit().expect("exit state");
    assert!(!exit.success);
    assert!(exit.signal.is_some(), "expected signal-kill, got {exit:?}");
}

#[test]
#[ignore = "drives the locally installed `claude` binary; run explicitly: cargo test -p terminal --test headless -- --ignored claude_tui"]
fn claude_tui_renders_a_styled_grid_headlessly() {
    // §11.4 Phase-4 gate: the `claude` TUI renders in the HEADLESS harness
    // (pty + emulator only — no gpui). Ignored by default: it needs a local,
    // configured `claude` install and its startup network probes make timing
    // machine-dependent.
    if std::process::Command::new("claude").arg("--version").output().is_err() {
        eprintln!("claude not found on PATH; skipping claude TUI smoke test");
        return;
    }
    let mut term = Terminal::spawn(&SpawnSpec::new("claude"), 100, 30).expect("spawn claude");

    // Poll for a non-empty, STYLED grid: claude's welcome banner paints
    // colored cells (non-default fg) within a few seconds. Styled-cell count
    // >= 20 rules out a bare shell error line.
    let styled_cells = |t: &Terminal| {
        let handle = t.term();
        let guard = handle.lock();
        let count = guard
            .grid
            .display_iter()
            .filter(|cell| {
                let square = *cell.square;
                let c = square.c();
                c != ' '
                    && c != '\0'
                    && guard.grid.style_of(&square).fg != AnsiColor::Named(NamedColor::Foreground)
            })
            .count();
        drop(guard);
        count
    };
    let rendered = pump_until(&mut term, Duration::from_secs(15), |t| {
        styled_cells(t) >= 20 && t.screen_lines().iter().any(|line| !line.trim().is_empty())
    });
    assert!(
        rendered,
        "claude never rendered a styled TUI grid (styled cells: {}):\n{}",
        styled_cells(&term),
        dump(&term)
    );
    assert!(term.is_running(), "claude exited before the TUI settled:\n{}", dump(&term));

    // Kill cleanly and confirm the exit edge is captured (bounded teardown).
    kill_until_reaped(&mut term, Duration::from_secs(5)).unwrap();
    assert!(!term.is_running());
}

#[test]
fn sixel_from_a_real_child_places_an_image() {
    // EXP-636: an image protocol stream from a real child ends up as a
    // placement in the emulator's graphics store. rio silently drops sixels
    // when the cell pixel metrics are zero, so this also guards the
    // `DEFAULT_CELL_PX` wiring end to end (the CLI path never sets real ones).
    let mut term = Terminal::spawn(&bash_spec(), 80, 24).expect("spawn bash");
    // A 6x6 solid red sixel: raster attributes, one palette entry, six full
    // sixel columns. The newline after ST moves the cursor off the image row:
    // sixels carry DEC semantics, so text written over them erases them.
    term.write(b"printf '\\033Pq\"1;1;6;6#0;2;100;0;0#0~~~~~~\\033\\\\\\n'; echo sixel-'d'one\n");
    assert!(
        pump_until(&mut term, LONG, |t| output_line_contains(t, "sixel-done")),
        "sixel command never finished:\n{}",
        dump(&term)
    );
    let handle = term.term();
    let placements = handle.lock().graphics.atlas_placements.len();
    assert_eq!(placements, 1, "expected one sixel placement:\n{}", dump(&term));
}

#[test]
fn vim_smoke_alt_screen_and_quit() {
    // Feasibility guard: skip (green) when vim isn't installed.
    if std::process::Command::new("vim").arg("--version").output().is_err() {
        eprintln!("vim not found on PATH; skipping vim smoke test");
        return;
    }
    let mut term = Terminal::spawn(
        &SpawnSpec::new("vim").args(["-u", "NONE", "-i", "NONE", "-n"]),
        80,
        24,
    )
    .expect("spawn vim");

    // Full-screen TUI entered the alt buffer and drew its empty-buffer
    // tildes. vim probes the terminal at startup (DA/DSR) — reaching the alt
    // screen at all proves pump()'s reply path (§6.6).
    let entered = pump_until(&mut term, LONG, |t| {
        let on_alt = {
            let handle = t.term();
            let mode = handle.lock().mode();
            mode.contains(TermMode::ALT_SCREEN)
        };
        on_alt && t.screen_lines().iter().any(|line| line.starts_with('~'))
    });
    assert!(entered, "vim never entered the alt screen:\n{}", dump(&term));

    // :q! and confirm a clean exit.
    term.write(b"\x1b:q!\r");
    assert!(pump_until(&mut term, LONG, |t| t.exit().is_some()), "vim never exited");
    assert!(term.exit().expect("exit state").success, "vim exited non-zero");
}
