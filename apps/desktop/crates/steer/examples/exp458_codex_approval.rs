//! EXP-458 manual smoke test: the codex approval overlay against a REAL
//! `codex` TUI run. The `codex_approval_picker` anchors were derived from the
//! codex-cli 0.144.5 source + committed render snapshots, never a live run —
//! this harness closes that gap and settles the one case the repo alone
//! could not: whether a resolved modal leaves detect()-matching text on the
//! visible screen (which would let a stale remote answer's digit reach the
//! composer past `handle_approval_answer`'s live-grid re-check).
//!
//! Run: `cargo run -p steer --example exp458_codex_approval` (needs a
//! logged-in `codex` >= 0.144.5 on PATH, or `EXP458_CODEX=/path/to/codex`;
//! spends a few tokens).
//!
//! The session runs the production GUARDED preset (`--sandbox
//! workspace-write --ask-for-approval on-request`, argv.rs) and the prompt
//! forces two out-of-workspace writes, so codex raises two exec approvals:
//!
//!   1. answered REMOTE-style — the same digit → 500ms probe → Enter
//!      choreography `handle_approval_answer` uses;
//!   2. answered LOCALLY (digit at the "TUI") — then the harness watches the
//!      visible screen for [`STALE_WATCH`] and asserts `detect` never fires
//!      again, i.e. a stale remote answer would keep getting `Retry` and its
//!      digit can never be injected;
//!
//! and finally types a probe string into the idle composer (erased again) to
//! document the counterfactual: keystrokes at that moment DO land in the
//! composer — the live-grid re-check is the only guard, and it holds.
//!
//! PASS criteria, printed at the end:
//!   1. both approval overlays detected (anchors valid on the live codex)
//!   2. both marker files exist (the answers actuated the modals)
//!   3. zero detect() hits during both post-resolution stale watches
//!   4. the trust dialog (numbered picker, no anchor title) never detected

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use steer::codex_approval_picker::{self, CodexApprovalWatcher, Transition};
use terminal::{display_offset, screen_lines, SpawnSpec, Terminal};

const STALE_WATCH: Duration = Duration::from_secs(15);
const DEADLINE: Duration = Duration::from_secs(420);
/// `handle_approval_answer`'s digit→Enter probe window (PLAN_SUBMIT_PROBE).
const SUBMIT_PROBE: Duration = Duration::from_millis(500);

fn main() {
    let start = Instant::now();
    let stamp = move || format!("[{:6.1}s]", start.elapsed().as_secs_f32());
    let home = std::env::var("HOME").expect("HOME set");
    let marker_one = PathBuf::from(&home).join("exp458-marker-one.txt");
    let marker_two = PathBuf::from(&home).join("exp458-marker-two.txt");
    let _ = std::fs::remove_file(&marker_one);
    let _ = std::fs::remove_file(&marker_two);

    // A scratch git worktree, so the workspace-write sandbox has a workspace
    // and the out-of-workspace `touch` genuinely needs escalation.
    let worktree = std::env::temp_dir().join(format!("exp458-worktree-{}", std::process::id()));
    std::fs::create_dir_all(&worktree).expect("create worktree");
    std::fs::write(worktree.join("README.md"), "exp458 scratch\n").expect("seed");
    for args in [
        vec!["init"],
        vec!["add", "README.md"],
        vec!["commit", "-m", "baseline"],
    ] {
        let ok = Command::new("git")
            .arg("-C")
            .arg(&worktree)
            .args(&args)
            .env("GIT_AUTHOR_NAME", "exp458")
            .env("GIT_AUTHOR_EMAIL", "exp458@example.com")
            .env("GIT_COMMITTER_NAME", "exp458")
            .env("GIT_COMMITTER_EMAIL", "exp458@example.com")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        assert!(ok, "git {args:?} failed");
    }

    let codex = std::env::var("EXP458_CODEX").unwrap_or_else(|_| "codex".to_string());
    let prompt = format!(
        "You have my explicit permission for everything below. First run exactly \
         `touch {one}` as its own command — the path is outside your workspace, so when \
         the sandbox blocks it, request escalated permissions and run it. After it \
         succeeds, run exactly `touch {two}` as a second separate command, again with \
         escalated permissions. Do not combine them, do not use apply_patch, do not do \
         anything else. Then say done and wait.",
        one = marker_one.display(),
        two = marker_two.display(),
    );
    println!("{} spawning {codex} (guarded preset) in a 100x30 PTY", stamp());
    // The production guarded preset from coding/src/argv.rs.
    let spec = SpawnSpec::new(&codex)
        .args([
            "--sandbox".to_string(),
            "workspace-write".to_string(),
            "--ask-for-approval".to_string(),
            "on-request".to_string(),
            "-c".to_string(),
            "sandbox_workspace_write.network_access=true".to_string(),
            prompt,
        ])
        .env("TERM", "xterm-256color")
        .cwd(worktree.to_str().unwrap());
    let terminal = Terminal::spawn(&spec, 100, 30).expect("spawn codex");
    let term = terminal.term();

    let mut watcher = CodexApprovalWatcher::new();
    let mut trusted = false;
    let mut trust_dialog_false_positive = false;
    let mut shows: Vec<codex_approval_picker::ApprovalSnapshot> = Vec::new();
    let mut stale_hits = 0u32;
    let mut stale_watch_done = 0u32;
    // A running stale watch: (deadline, the snapshot that just resolved).
    // Only a re-detect EQUAL to the resolved snapshot is a ghost — a
    // different one is the next genuine modal (back-to-back approvals) and
    // ends the watch so the watcher can Show it.
    let mut stale_watch: Option<(Instant, codex_approval_picker::ApprovalSnapshot)> = None;
    let mut last_dump = Instant::now();

    let inject_answer = |label: &str, before: &codex_approval_picker::ApprovalSnapshot| {
        // The `handle_approval_answer` choreography: the digit actuates the
        // list; Enter only as the safety net if the snapshot did not move.
        terminal.write(b"1");
        let probe_end = Instant::now() + SUBMIT_PROBE;
        let mut moved = false;
        while Instant::now() < probe_end {
            std::thread::sleep(Duration::from_millis(50));
            match codex_approval_picker::detect(&screen_lines(&term)) {
                None => {
                    moved = true;
                    break;
                }
                Some(next) if &next != before => {
                    moved = true;
                    break;
                }
                Some(_) => {}
            }
        }
        if !moved {
            terminal.write(b"\r");
            println!("[answer:{label}] digit did not move the modal — Enter safety net fired");
        } else {
            println!("[answer:{label}] digit actuated the modal (no Enter needed)");
        }
    };

    let deadline = Instant::now() + DEADLINE;
    while Instant::now() < deadline {
        let lines = screen_lines(&term);
        let offset = display_offset(&term);

        if !trusted {
            if lines
                .iter()
                .any(|line| line.contains("Do you trust the contents of this directory"))
            {
                // The trust picker is numbered + cursor-marked like the
                // approval overlay but has no anchor title — it must never
                // trip the detector.
                if codex_approval_picker::detect(&lines).is_some() {
                    trust_dialog_false_positive = true;
                    println!("{} !! trust dialog DETECTED as an approval", stamp());
                }
                trusted = true;
                terminal.write(b"\r");
                println!("{} accepted codex trust dialog", stamp());
                std::thread::sleep(Duration::from_millis(300));
                continue;
            }
        }

        // A running stale watch: the resolved modal must never re-detect.
        if let Some((until, resolved)) = &stale_watch {
            let mut finished = Instant::now() >= *until;
            match codex_approval_picker::detect(&lines) {
                Some(ghost) if &ghost == resolved => {
                    stale_hits += 1;
                    println!(
                        "{} !! STALE DETECT HIT: {:?} ({} options)",
                        stamp(),
                        ghost.title,
                        ghost.options.len()
                    );
                }
                Some(next) => {
                    // The NEXT genuine modal painted — hand it back to the
                    // watcher (its changed-dialog Show path owns this).
                    println!("{} next modal painted ({:?}) — stale watch ends", stamp(), next.title);
                    finished = true;
                }
                None => {}
            }
            if finished {
                stale_watch = None;
                stale_watch_done += 1;
                println!(
                    "{} stale watch {} done — {} ghost detect() hits so far",
                    stamp(),
                    stale_watch_done,
                    stale_hits
                );
                if stale_watch_done == 2 {
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(100));
            continue;
        }

        match watcher.tick(&lines, offset) {
            Some(Transition::Show(snapshot)) => {
                println!("{} ---- approval overlay {} settled ----", stamp(), shows.len() + 1);
                println!("| title: {}", snapshot.title);
                for line in &snapshot.context {
                    println!("| context: {line}");
                }
                for option in &snapshot.options {
                    println!("| option {}: {}", option.key, option.label);
                }
                let label = if shows.is_empty() { "remote" } else { "local" };
                inject_answer(label, &snapshot);
                shows.push(snapshot);
            }
            Some(Transition::Resolved) => {
                println!("{} overlay resolved — starting {}s stale watch", stamp(), STALE_WATCH.as_secs());
                let resolved = shows.last().expect("a Show preceded the Resolved").clone();
                stale_watch = Some((Instant::now() + STALE_WATCH, resolved));
            }
            None => {}
        }

        if last_dump.elapsed() >= Duration::from_secs(30) {
            last_dump = Instant::now();
            println!("{} ---- grid ----", stamp());
            for line in lines.iter().filter(|l| !l.trim().is_empty()) {
                println!("| {line}");
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    // The counterfactual: with no modal up, keystrokes land in the composer —
    // demonstrating what a stale injection WOULD do without the live-grid
    // re-check. Typed and erased again.
    let probe = "exp458probe";
    terminal.write(probe.as_bytes());
    std::thread::sleep(Duration::from_millis(700));
    let composer_probe = screen_lines(&term).iter().any(|line| line.contains(probe));
    for _ in 0..probe.len() {
        terminal.write(b"\x7f");
    }
    std::thread::sleep(Duration::from_millis(300));
    let probe_erased = !screen_lines(&term).iter().any(|line| line.contains(probe));

    println!("\n===== final grid =====");
    for line in screen_lines(&term).iter().filter(|l| !l.trim().is_empty()) {
        println!("| {line}");
    }
    println!("======================");
    let one = marker_one.exists();
    let two = marker_two.exists();
    let pass = shows.len() >= 2
        && one
        && two
        && stale_hits == 0
        && stale_watch_done >= 2
        && !trust_dialog_false_positive;
    println!(
        "[result] {} — overlays:{} marker-one(remote answer):{one} marker-two(local answer):{two} \
         stale-watches:{stale_watch_done} stale-detect-hits:{stale_hits} \
         trust-false-positive:{trust_dialog_false_positive} \
         composer-probe-landed:{composer_probe} composer-probe-erased:{probe_erased}",
        if pass { "PASS" } else { "FAIL" },
        shows.len(),
    );
    let _ = std::fs::remove_file(&marker_one);
    let _ = std::fs::remove_file(&marker_two);
    let _ = std::fs::remove_dir_all(&worktree);
    terminal.kill();
}
