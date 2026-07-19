// Clean reimplementation from the VT spec + alacritty_terminal (Apache-2.0). NOT derived from Zed's GPL terminal crates.
//! Headless integration tests for the gpui-free terminal core (masterplan-v3
//! §6.2 / §11.4 Phase-4 gate): real PTY, real children (`bash`, `sh`, `vim`),
//! grid-level assertions — no window, no gpui.

use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::TermMode;
use std::sync::Arc;
use std::time::{Duration, Instant};
use terminal::{CaptureSink, RawSink, SpawnSpec, Terminal};

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
fn tee_sink_receives_raw_bytes_alongside_emulator() {
    let mut term = Terminal::spawn(&bash_spec(), 80, 24).expect("spawn bash");
    let capture = Arc::new(CaptureSink::new());
    let sink: Arc<dyn RawSink> = capture.clone();
    term.attach_sink(sink.clone());

    term.write(b"echo tee-'m'arker\n");
    assert!(
        pump_until(&mut term, LONG, |t| output_line_contains(t, "tee-marker")),
        "emulator never rendered the marker:\n{}",
        dump(&term)
    );

    // The SAME single read fed both consumers (§6.4 / gate #8): the emulator
    // rendered the marker above, and the raw sink saw the identical bytes —
    // including the PTY's ONLCR-emitted \r\n (no fixup applied, §6.4).
    let raw = String::from_utf8_lossy(&capture.bytes()).into_owned();
    assert!(raw.contains("tee-marker\r\n"), "raw tee missing marker+CRLF: {raw:?}");

    // Detach (§6.14): later output must not reach the sink.
    term.detach_sink(&sink);
    term.write(b"echo post-'d'etach\n");
    assert!(
        pump_until(&mut term, LONG, |t| output_line_contains(t, "post-detach")),
        "grid never showed post-detach output:\n{}",
        dump(&term)
    );
    let raw_after = String::from_utf8_lossy(&capture.bytes()).into_owned();
    assert!(!raw_after.contains("post-detach"), "detached sink still fed: {raw_after:?}");
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
    // to alacritty (which rejects zero grids) or the child (SIGWINCH storm).
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
    // with WIDE_CHAR and the trailing cell is a WIDE_CHAR_SPACER.
    let handle = term.term();
    let guard = handle.lock();
    let cells: Vec<(i32, usize, char, Flags)> = guard
        .renderable_content()
        .display_iter
        .map(|cell| (cell.point.line.0, cell.point.column.0, cell.c, cell.flags))
        .collect();
    drop(guard);

    for wide in ['你', '好', '🌍'] {
        let (line, column, _, flags) = *cells
            .iter()
            .find(|(_, _, c, _)| *c == wide)
            .unwrap_or_else(|| panic!("{wide} not in grid"));
        assert!(flags.contains(Flags::WIDE_CHAR), "{wide} missing WIDE_CHAR flag");
        let (.., spacer_flags) = *cells
            .iter()
            .find(|(l, col, _, _)| *l == line && *col == column + 1)
            .unwrap_or_else(|| panic!("no cell after {wide}"));
        assert!(
            spacer_flags.contains(Flags::WIDE_CHAR_SPACER),
            "cell after {wide} is not a WIDE_CHAR_SPACER"
        );
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
    term.kill();
    assert!(pump_until(&mut term, LONG, |t| t.exit().is_some()), "kill never reaped");
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
            .renderable_content()
            .display_iter
            .filter(|cell| {
                cell.c != ' '
                    && cell.fg != vte::ansi::Color::Named(vte::ansi::NamedColor::Foreground)
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
    term.kill();
    assert!(pump_until(&mut term, LONG, |t| t.exit().is_some()), "claude kill never reaped");
    assert!(!term.is_running());
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
            let mode = *handle.lock().mode();
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
