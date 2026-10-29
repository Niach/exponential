//! Where claude keeps a conversation (EXP-1426 part 4c, out of
//! `coding::launcher`). Read-only: the file is found, never opened here.

use std::path::{Path, PathBuf};

/// EXP-761: the ONE way to find a claude transcript — `<root>/*/<session
/// id>.jsonl`, `root` being `~/.claude/projects` (or the child's
/// `CLAUDE_CONFIG_DIR/projects`). Claude names the per-cwd directory by
/// munging the cwd (every non-alphanumeric byte → `-`), but caps the name at
/// roughly 200 chars and appends a 6-char hash beyond that
/// (`…-actions-5ecb3c42-…--71h4ur`), so recomputing the name from the cwd
/// misses exactly the long worktree paths a daemon data dir produces. The
/// session uuid is unique across the tree, so the first hit IS the
/// transcript; nothing here depends on the naming rule. Steer's PTY emitter,
/// the resume probe and the engine's replay all resolve through this
/// (`coding::launcher` re-exports it).
pub fn locate_claude_transcript(root: &Path, session_id: &str) -> Option<PathBuf> {
    let file_name = format!("{session_id}.jsonl");
    std::fs::read_dir(root)
        .ok()?
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| entry.path().join(&file_name))
        .find(|path| path.is_file())
}
