//! #EXP-1658 — a session's TEAM SKILLS, delivered to the agent CLI
//! (`packages/domain-contract/fixtures/skills.json`). Not to be confused
//! with [`crate::skill`], the run playbook.
//!
//! At spawn AND on every resume the launcher asks `skills.resolveForLaunch`
//! (named by `X-Exp-Session-Id`, like `mcpServers.resolveForLaunch`) for the
//! session's pick and writes it to `<data_dir>/skills/<session>/` — a
//! launcher-owned folder OUTSIDE the repo, the worktree, the scratch dir and
//! the agent home (the [`crate::skill`] header's rule: nothing new lands in
//! the user's files). Layout:
//!
//! ```text
//! <data_dir>/skills/<session>/.claude/skills/<name>/SKILL.md
//! ```
//!
//! where SKILL.md = YAML frontmatter (`name`, `description`) then the body.
//! A team skill has no files beside SKILL.md; a BUILT-IN (#VAPP-213) brings
//! its references and examples, written at `<name>/<path>` (a path that is
//! not `domain::skills::is_skill_file_path` is a warning, never written).
//!
//! **Claude** gets the folder through `--add-dir <data_dir>/skills/<session>`.
//! SPIKE (claude 2.1.317 on mint, 2026-11-03): `--add-dir` loads
//! `<dir>/.claude/skills/*` UNPREFIXED (the init frame listed `zebra-check`),
//! while the same skill as a `--plugin-dir` plugin named `team` lists as
//! `team:zebra-plug`. The unprefixed way won, so there is no
//! `.claude-plugin/plugin.json`. (`--add-dir` also widens the CLI's own file
//! access to this folder, which holds nothing but the skills; Exponential's
//! scratch confinement is its own hook and is unaffected.)
//!
//! **Codex** (app-server, 0.156) gets `skills/extraRoots/set {extraRoots:
//! [<dir>/.claude/skills]}` before `thread/start` / `thread/resume`; the
//! spike's `skills/list` listed the skill (scope `user`). Both CLIs parse the
//! JSON-quoted description (a YAML double-quoted scalar) verbatim.
//!
//! **The agent shell** (an interactive terminal tab) is UNSUPPORTED: it runs
//! no `sessions` row, so there is no pick to resolve.
//!
//! **A resolve never blocks a launch**: a failed call is a logged warning and
//! the run starts without skills; a picked skill deleted since is skipped.
//! The folder is purged when the run ends ([`purge`], called by the hosts
//! where they reap a run) and by the startup [`sweep`].

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use api::skills::{resolve_for_launch, LaunchSkill, SkillLaunchResolution};
use api::trpc::TrpcClient;

/// Dirs younger than this are never swept (the prepare → register window).
pub const GRACE: Duration = crate::prune::LAUNCH_GRACE;

/// `<data_dir>/skills`.
pub fn skills_root(data_dir: &Path) -> PathBuf {
    data_dir.join("skills")
}

/// `<data_dir>/skills/<session>`: the folder claude's `--add-dir` names.
/// `None` for a session id that is not one plain path segment.
pub fn session_dir(data_dir: &Path, session_id: &str) -> Option<PathBuf> {
    is_plain_segment(session_id).then(|| skills_root(data_dir).join(session_id))
}

/// The root codex's `skills/extraRoots/set` names: `<dir>/.claude/skills`.
pub fn codex_root(dir: &Path) -> PathBuf {
    dir.join(".claude").join("skills")
}

fn is_plain_segment(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\', '\0'])
}

/// Claude's skill-name rule (`limits.namePattern` `^[a-z0-9-]+$`, at most
/// `limits.nameMax`): the server enforces it; the device re-checks because
/// the name becomes a directory.
pub fn is_valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// One launch's resolved skills.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ResolvedSkills {
    /// The written folder (`--add-dir`); `None` = no skills this run.
    pub dir: Option<PathBuf>,
    /// The pick the resolve answered for: every id resolved or skipped,
    /// sorted and deduped (`sessions.skillPicks`' shape). `None` = not
    /// resolved (the call failed): a live run never compares it.
    pub pick: Option<Vec<String>>,
    /// Launch-time notes; never a blocker.
    pub warnings: Vec<String>,
}

/// The SKILL.md text: frontmatter (`name`, then `description` JSON-quoted,
/// which is a valid YAML double-quoted scalar) and the body.
pub fn skill_md(skill: &LaunchSkill) -> String {
    let description = serde_json::to_string(&skill.description).unwrap_or_else(|_| "\"\"".to_string());
    let mut text = format!("---\nname: {}\ndescription: {description}\n---\n\n{}", skill.name, skill.body);
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

/// Resolve the session's pick and write it ([`write`]). Every failure is a
/// warning; the stale folder of an earlier run of the session goes first.
pub fn resolve(trpc: &TrpcClient, data_dir: &Path, session_id: &str) -> ResolvedSkills {
    purge(data_dir, session_id);
    match resolve_for_launch(trpc, session_id) {
        Ok(resolution) => write(data_dir, session_id, &resolution),
        Err(error) => ResolvedSkills {
            warnings: vec![format!(
                "Could not load the session's skills ({}); starting without them.",
                error.user_message()
            )],
            ..ResolvedSkills::default()
        },
    }
}

/// Write `resolution` to [`session_dir`] (nothing for an empty pick). A
/// skill with an invalid or duplicate name, and a write failure, are
/// warnings; a run whose folder could not be written runs without skills.
pub fn write(data_dir: &Path, session_id: &str, resolution: &SkillLaunchResolution) -> ResolvedSkills {
    let mut pick: Vec<String> = resolution
        .skills
        .iter()
        .map(|skill| skill.id.clone())
        .chain(resolution.skipped.iter().cloned())
        .filter(|id| !id.is_empty())
        .collect();
    pick.sort_unstable();
    pick.dedup();
    let mut resolved = ResolvedSkills {
        pick: Some(pick),
        ..ResolvedSkills::default()
    };
    if !resolution.skipped.is_empty() {
        resolved.warnings.push(format!(
            "{} picked skill(s) no longer exist; starting without them.",
            resolution.skipped.len()
        ));
    }
    if resolution.skills.is_empty() {
        return resolved;
    }
    let Some(dir) = session_dir(data_dir, session_id) else {
        resolved.warnings.push(format!("skills: session id {session_id:?} is not a path segment; starting without skills."));
        return resolved;
    };
    let root = codex_root(&dir);
    let mut written = 0usize;
    let mut seen = std::collections::HashSet::new();
    for skill in &resolution.skills {
        if !is_valid_name(&skill.name) || !seen.insert(skill.name.as_str()) {
            resolved.warnings.push(format!("skills: skipped the skill named {:?} (invalid or duplicate name).", skill.name));
            continue;
        }
        let skill_dir = root.join(&skill.name);
        let result = std::fs::create_dir_all(&skill_dir)
            .and_then(|()| std::fs::write(skill_dir.join("SKILL.md"), skill_md(skill)))
            .and_then(|()| write_files(&skill_dir, skill, &mut resolved.warnings));
        match result {
            Ok(()) => written += 1,
            Err(err) => resolved
                .warnings
                .push(format!("skills: could not write {}: {err}", skill_dir.display())),
        }
    }
    if written > 0 {
        resolved.dir = Some(dir);
    } else {
        purge(data_dir, session_id);
    }
    resolved
}

/// A built-in's files beside its SKILL.md; an unsafe path is skipped with a
/// warning (the server only sends generated, relative paths).
fn write_files(skill_dir: &Path, skill: &LaunchSkill, warnings: &mut Vec<String>) -> std::io::Result<()> {
    for file in &skill.files {
        if !domain::skills::is_skill_file_path(&file.path) {
            warnings.push(format!("skills: skipped the file {:?} of {:?} (not a relative path).", file.path, skill.name));
            continue;
        }
        let target = file.path.split('/').fold(skill_dir.to_path_buf(), |at, segment| at.join(segment));
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(target, &file.content)?;
    }
    Ok(())
}

/// Remove the session's skills folder. Best-effort; returns whether a
/// folder went.
pub fn purge(data_dir: &Path, session_id: &str) -> bool {
    let Some(dir) = session_dir(data_dir, session_id) else {
        return false;
    };
    match std::fs::remove_dir_all(&dir) {
        Ok(()) => true,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => false,
        Err(err) => {
            log::warn!("skills purge: remove {}: {err}", dir.display());
            false
        }
    }
}

/// The startup pass: every session folder under [`skills_root`] that no live
/// session owns and that is older than [`GRACE`] goes. Returns how many.
pub fn sweep(data_dir: &Path, live_session_ids: &[String]) -> usize {
    let now = SystemTime::now();
    let Ok(entries) = std::fs::read_dir(skills_root(data_dir)) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if live_session_ids.iter().any(|id| *id == name) {
            continue;
        }
        let young = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age < GRACE)
            // A clock that went backwards counts as young.
            || entry
                .metadata()
                .and_then(|meta| meta.modified())
                .is_ok_and(|modified| modified > now);
        if young {
            continue;
        }
        if std::fs::remove_dir_all(entry.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skill(id: &str, name: &str) -> LaunchSkill {
        LaunchSkill {
            id: id.into(),
            name: name.into(),
            description: format!("Use for {name}: \"quoted\""),
            body: "# Steps\n\nDo it.".into(),
            files: Vec::new(),
        }
    }

    #[test]
    fn a_builtin_writes_its_files_beside_skill_md_and_skips_unsafe_paths() {
        let data = crate::test_support::temp_dir("skills-files");
        let mut builtin = skill("k3", "exponential-apps");
        builtin.files = vec![
            api::skills::LaunchSkillFile { path: "references/records.md".into(), content: "# Records\n".into() },
            api::skills::LaunchSkillFile { path: "examples/crm.app.json".into(), content: "{}\n".into() },
            api::skills::LaunchSkillFile { path: "../escape.md".into(), content: "no".into() },
        ];
        let resolution = SkillLaunchResolution { skills: vec![builtin], skipped: vec![] };
        let resolved = write(data.0.as_path(), "sess-f", &resolution);
        let root = codex_root(resolved.dir.as_ref().unwrap()).join("exponential-apps");
        assert!(root.join("SKILL.md").is_file());
        assert_eq!(std::fs::read_to_string(root.join("references/records.md")).unwrap(), "# Records\n");
        assert_eq!(std::fs::read_to_string(root.join("examples/crm.app.json")).unwrap(), "{}\n");
        assert!(!codex_root(resolved.dir.as_ref().unwrap()).join("escape.md").exists());
        assert!(!data.0.as_path().join("skills").join("escape.md").exists());
        assert_eq!(resolved.warnings.len(), 1, "{:?}", resolved.warnings);
    }

    #[test]
    fn skill_md_is_frontmatter_then_the_body() {
        let text = skill_md(&skill("k1", "review-checklist"));
        assert_eq!(
            text,
            "---\nname: review-checklist\ndescription: \"Use for review-checklist: \\\"quoted\\\"\"\n---\n\n# Steps\n\nDo it.\n"
        );
    }

    #[test]
    fn write_lays_out_one_skill_dir_per_pick_outside_the_worktree() {
        let data = crate::test_support::temp_dir("skills");
        let resolution = SkillLaunchResolution {
            skills: vec![skill("k2", "zeta"), skill("k1", "alpha")],
            skipped: vec!["k9".into()],
        };
        let resolved = write(data.0.as_path(), "sess-1", &resolution);
        let dir = data.0.as_path().join("skills").join("sess-1");
        assert_eq!(resolved.dir.as_deref(), Some(dir.as_path()));
        assert_eq!(resolved.pick, Some(vec!["k1".into(), "k2".into(), "k9".into()]));
        assert_eq!(resolved.warnings.len(), 1, "the skipped pick is a warning");
        let md = std::fs::read_to_string(dir.join(".claude/skills/alpha/SKILL.md")).unwrap();
        assert!(md.starts_with("---\nname: alpha\n"));
        assert!(dir.join(".claude/skills/zeta/SKILL.md").is_file());
        assert_eq!(codex_root(&dir), dir.join(".claude/skills"));
        assert!(!dir.join(".claude-plugin").exists(), "unprefixed: no plugin manifest");
    }

    #[test]
    fn an_empty_pick_writes_nothing_and_a_bad_name_never_escapes() {
        let data = crate::test_support::temp_dir("skills");
        let empty = write(data.0.as_path(), "sess-1", &SkillLaunchResolution::default());
        assert_eq!(empty, ResolvedSkills { pick: Some(Vec::new()), ..ResolvedSkills::default() });
        assert!(!data.0.as_path().join("skills").exists());

        let hostile = SkillLaunchResolution {
            skills: vec![skill("k1", "../../escape"), skill("k2", "Upper")],
            skipped: Vec::new(),
        };
        let resolved = write(data.0.as_path(), "sess-1", &hostile);
        assert_eq!(resolved.dir, None, "nothing valid = no skills dir");
        assert_eq!(resolved.warnings.len(), 2);
        assert!(!data.0.as_path().join("escape").exists());
        assert!(!data.0.as_path().join("skills").join("sess-1").exists());
        assert_eq!(write(data.0.as_path(), "../x", &SkillLaunchResolution { skills: vec![skill("k1", "a")], skipped: vec![] }).dir, None);
    }

    #[test]
    fn a_failed_resolve_is_a_warning_and_purges_the_stale_folder() {
        let data = crate::test_support::temp_dir("skills");
        write(data.0.as_path(), "sess-1", &SkillLaunchResolution { skills: vec![skill("k1", "a")], skipped: vec![] });
        assert!(data.0.as_path().join("skills/sess-1").is_dir());
        let trpc = TrpcClient::new("http://127.0.0.1:9", std::sync::Arc::new(|| None));
        let resolved = resolve(&trpc, data.0.as_path(), "sess-1");
        assert_eq!(resolved.dir, None);
        assert_eq!(resolved.pick, None, "unknown, never compared");
        assert_eq!(resolved.warnings.len(), 1);
        assert!(!data.0.as_path().join("skills/sess-1").exists());
    }

    #[test]
    fn purge_and_sweep_remove_ended_runs_only() {
        let data = crate::test_support::temp_dir("skills");
        let one = SkillLaunchResolution { skills: vec![skill("k1", "a")], skipped: vec![] };
        write(data.0.as_path(), "live", &one);
        write(data.0.as_path(), "ended", &one);
        // Young dirs survive the sweep (the prepare → register window).
        assert_eq!(sweep(data.0.as_path(), &["live".into()]), 0);
        assert!(purge(data.0.as_path(), "ended"));
        assert!(!purge(data.0.as_path(), "ended"));
        assert!(data.0.as_path().join("skills/live").is_dir());
        assert!(!purge(data.0.as_path(), ".."), "never outside the root");
    }
}
