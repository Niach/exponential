//! The merge train's engine on git argv (#EXP-1617, feature `compose`): a
//! port of `scripts/merge-train/compose.ts` (fixtures/merge-train.json
//! `pipeline`), so an `ecode` repository's train composes a batch exactly
//! like the GitHub one, generated-file resolver rules included. Generic: it
//! knows git and the repo's `.exponential/merge-train.json`, nothing of a
//! forge. From `base_sha`, each PR of each unit in queue order is
//! squash-merged onto the train tip; conflicted paths go through the repo's
//! resolver rules; a unit that cannot be resolved is dropped whole (the tip
//! resets to before it) and composing goes on.
//!
//! The pure half (globs, `resolve_conflicts`, `train_checks_mode`, the file
//! strategies) is `ecode_core::train`. Repo commands (`setup`, a rule's
//! `command`) run through an injectable runner, by default
//! `bash -euo pipefail -c <command>` in the work tree.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use ecode_core::{
    file_number, json_union, prepend_union, resolve_conflicts, rule_of, same_contents, train_checks_mode, train_glob_match,
    Against, ChecksModeInput, ResolveStrategy, ResolveVerdict, TrainChecksMode, TrainMode, TrainPipelineOutcome, TrainPipelineUnit,
    TrainRepoConfig, TrainUnitOutcome, UnitOutcome,
};

use crate::{Error, GitCommand, Result};

/// The repo's policy file, read at the BASE (never a unit's).
pub const TRAIN_CONFIG_FILE: &str = ".exponential/merge-train.json";

/// What to compose.
#[derive(Clone, Debug)]
pub struct ComposeInput {
    pub train_id: String,
    pub base_sha: String,
    pub mode: TrainMode,
    pub units: Vec<TrainPipelineUnit>,
    /// `None` = the base's own `.exponential/merge-train.json`.
    pub config: Option<TrainRepoConfig>,
    /// `(name, email)`: the committer (and default author) of the train's
    /// commits; `None` = git's own configuration.
    pub identity: Option<(String, String)>,
}

/// The outcome + the CI workflows the caller starts on the head
/// (`checks` = dispatched).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComposeResult {
    pub outcome: TrainPipelineOutcome,
    pub dispatch: Vec<String>,
}

/// Runs a repo command in a directory: `true` = it succeeded.
pub type RunCommand<'a> = &'a dyn Fn(&str, &Path) -> bool;

/// The default runner: `bash -euo pipefail -c <command>` in `dir`.
pub fn run_shell(command: &str, dir: &Path) -> bool {
    match Command::new("bash").args(["-euo", "pipefail", "-c", command]).current_dir(dir).output() {
        Ok(out) if out.status.success() => true,
        Ok(out) => {
            let err = String::from_utf8_lossy(if out.stderr.is_empty() { &out.stdout } else { &out.stderr }).into_owned();
            eprintln!("[merge-train] `{command}` failed:\n{err}");
            false
        }
        Err(e) => {
            eprintln!("[merge-train] `{command}` did not start: {e}");
            false
        }
    }
}

/// The outcome of a compose that failed (cli.ts's catch): no head, no CI.
pub fn error_outcome(train_id: &str, base_sha: &str, message: &str) -> TrainPipelineOutcome {
    TrainPipelineOutcome {
        train: train_id.to_string(),
        base_sha: base_sha.to_string(),
        head: None,
        checks: TrainChecksMode::None,
        units: vec![],
        error: Some(message.chars().take(2000).collect()),
    }
}

/// `compose` with [`run_shell`].
pub fn compose(dir: &Path, input: &ComposeInput) -> Result<ComposeResult> {
    compose_with(dir, input, &run_shell)
}

enum Fail {
    /// The files that bounce the unit.
    Conflict(Vec<String>),
    Error(Error),
}

impl From<Error> for Fail {
    fn from(e: Error) -> Self {
        Fail::Error(e)
    }
}

type Step<T> = std::result::Result<T, Fail>;

struct Run {
    ok: bool,
    out: String,
}

struct Git {
    dir: PathBuf,
    identity: Option<(String, String)>,
}

impl Git {
    fn command(&self, args: &[&str]) -> GitCommand {
        let mut c = GitCommand::new(Some(&self.dir)).args(["-c", "commit.gpgsign=false"]);
        if let Some((name, email)) = &self.identity {
            c = c
                .env("GIT_AUTHOR_NAME", name)
                .env("GIT_AUTHOR_EMAIL", email)
                .env("GIT_COMMITTER_NAME", name)
                .env("GIT_COMMITTER_EMAIL", email);
        }
        c.args(args)
    }

    fn run(&self, args: &[&str]) -> Result<Run> {
        let out = self.command(args).output()?;
        Ok(Run { ok: out.success, out: out.stdout })
    }

    fn must(&self, args: &[&str]) -> Result<String> {
        Ok(self.command(args).run()?.trim().to_string())
    }

    fn lines(&self, args: &[&str]) -> Result<Vec<String>> {
        Ok(self.must(args)?.lines().filter(|l| !l.is_empty()).map(str::to_string).collect())
    }

    /// A path's content at stage 1 (base), 2 (ours) or 3 (theirs); None = absent.
    fn stage(&self, n: u8, path: &str) -> Result<Option<String>> {
        self.show(&format!(":{n}"), path)
    }

    fn show(&self, rev: &str, path: &str) -> Result<Option<String>> {
        let r = self.run(&["show", &format!("{rev}:{path}")])?;
        Ok(r.ok.then_some(r.out))
    }

    /// A path's blob id at `rev` (`""` = the index); None = absent.
    fn blob_at(&self, rev: &str, path: &str) -> Result<Option<String>> {
        let r = self.run(&["rev-parse", "--verify", "--quiet", &format!("{rev}:{path}")])?;
        Ok(r.ok.then(|| r.out.trim().to_string()))
    }
}

struct Engine<'a> {
    git: Git,
    config: TrainRepoConfig,
    run: RunCommand<'a>,
    setup_done: bool,
}

/// The repo's policy as the BASE declares it.
pub fn read_config(dir: &Path, base_sha: &str) -> Result<TrainRepoConfig> {
    let git = Git { dir: dir.to_path_buf(), identity: None };
    match git.show(base_sha, TRAIN_CONFIG_FILE)? {
        None => Ok(TrainRepoConfig::default()),
        Some(text) => serde_json::from_str(&text).map_err(|e| Error::Invalid(format!("{TRAIN_CONFIG_FILE}: {e}"))),
    }
}

/// The engine: `dir` = a work tree with base_sha and every head (and
/// tested_sha) present. Moves no ref; leaves the work tree at the head.
pub fn compose_with(dir: &Path, input: &ComposeInput, run: RunCommand<'_>) -> Result<ComposeResult> {
    let config = match &input.config {
        Some(config) => config.clone(),
        None => read_config(dir, &input.base_sha)?,
    };
    let mut e = Engine { git: Git { dir: dir.to_path_buf(), identity: input.identity.clone() }, config, run, setup_done: false };
    let base = input.base_sha.as_str();
    e.git.must(&["checkout", "--quiet", "--detach", base])?;
    e.git.must(&["reset", "--quiet", "--hard", base])?;

    let mut units: Vec<TrainUnitOutcome> = vec![];
    for unit in &input.units {
        let before = e.git.must(&["rev-parse", "HEAD"])?;
        let prs: Vec<u64> = unit.prs.iter().map(|p| p.number).collect();
        match e.compose_unit(unit) {
            Ok((resolved, drifted)) => units.push(TrainUnitOutcome {
                prs,
                outcome: UnitOutcome::Composed,
                files: vec![],
                resolved,
                against: None,
                drifted: (!drifted.is_empty()).then_some(drifted),
            }),
            Err(Fail::Error(err)) => return Err(err),
            Err(Fail::Conflict(files)) => {
                e.git.must(&["reset", "--quiet", "--hard", &before])?;
                e.git.must(&["clean", "-fdq"])?;
                // #EXP-1711: behind earlier units, does it compose on the base
                // alone? Then only a sibling that has not landed is in its way.
                let alone = before != base && e.composes_alone(base, unit)?;
                e.git.must(&["checkout", "--quiet", "--detach", &before])?;
                e.git.must(&["reset", "--quiet", "--hard", &before])?;
                e.git.must(&["clean", "-fdq"])?;
                units.push(TrainUnitOutcome {
                    prs,
                    outcome: UnitOutcome::Conflict,
                    files,
                    resolved: vec![],
                    against: alone.then_some(Against::Batch),
                    drifted: None,
                });
            }
        }
    }

    let composed = units.iter().any(|u| u.outcome == UnitOutcome::Composed);
    if !composed {
        e.git.must(&["commit", "--quiet", "--allow-empty", "-m", &format!("exp-train {}: nothing composed", input.train_id)])?;
    }
    let head = e.git.must(&["rev-parse", "HEAD"])?;
    let workflows = e.config.ci.as_ref().map(|ci| ci.workflows.clone()).unwrap_or_default();
    // #LOGS-2: a Merge now is untested only when nothing was resolved, no
    // lockfile drifted from its head and no PR is behind the base.
    let riding: Vec<&TrainUnitOutcome> = units.iter().filter(|u| u.outcome == UnitOutcome::Composed).collect();
    let mut behind = false;
    if input.mode == TrainMode::Untested {
        'units: for (i, unit) in input.units.iter().enumerate() {
            if units[i].outcome != UnitOutcome::Composed {
                continue;
            }
            for pr in &unit.prs {
                if !e.git.run(&["merge-base", "--is-ancestor", base, &pr.head_sha])?.ok {
                    behind = true;
                    break 'units;
                }
            }
        }
    }
    let proven = composed && e.proven(&input.units, &units, &head)?;
    let checks = train_checks_mode(&ChecksModeInput {
        mode: input.mode,
        composed,
        resolved: riding.iter().any(|u| !u.resolved.is_empty()),
        drifted: riding.iter().any(|u| u.drifted.as_ref().is_some_and(|d| !d.is_empty())),
        behind,
        proven,
        workflows: workflows.clone(),
    });
    Ok(ComposeResult {
        outcome: TrainPipelineOutcome {
            train: input.train_id.clone(),
            base_sha: input.base_sha.clone(),
            head: Some(head),
            checks,
            units,
            error: None,
        },
        dispatch: if checks == TrainChecksMode::Dispatched { workflows } else { vec![] },
    })
}

fn uniq(list: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    list.into_iter().filter(|x| seen.insert(x.clone())).collect()
}

impl Engine<'_> {
    fn setup(&mut self) -> bool {
        if self.setup_done {
            return true;
        }
        let Some(setup) = self.config.setup.clone() else { return true };
        self.setup_done = true;
        (self.run)(&setup, &self.git.dir)
    }

    /// Every PR of the unit squashed and committed: → the rule names
    /// applied + the drifted lockfiles.
    fn compose_unit(&mut self, unit: &TrainPipelineUnit) -> Step<(Vec<String>, Vec<String>)> {
        let (mut resolved, mut drifted) = (vec![], vec![]);
        for pr in &unit.prs {
            let (r, d) = self.squash(&pr.head_sha)?;
            resolved.extend(r);
            drifted.extend(d);
            if self.git.run(&["diff", "--cached", "--quiet"])?.ok {
                continue; // already in the base
            }
            let message = format!("{} (#{})", pr.title, pr.number);
            let mut args = vec!["commit", "--quiet", "--no-verify", "-m", &message];
            if let Some(author) = pr.author.as_deref().filter(|a| !a.is_empty()) {
                args.extend(["--author", author]);
            }
            self.git.must(&args)?;
        }
        Ok((uniq(resolved), uniq(drifted)))
    }

    /// #EXP-1711: the unit squashed onto the bare base, thrown away after.
    fn composes_alone(&mut self, base: &str, unit: &TrainPipelineUnit) -> Result<bool> {
        self.git.must(&["checkout", "--quiet", "--detach", base])?;
        self.git.must(&["reset", "--quiet", "--hard", base])?;
        let attempt = (|| -> Step<bool> {
            for pr in &unit.prs {
                self.squash(&pr.head_sha)?;
                if self.git.run(&["diff", "--cached", "--quiet"])?.ok {
                    continue;
                }
                self.git.must(&["commit", "--quiet", "--no-verify", "-m", &format!("probe #{}", pr.number)])?;
            }
            Ok(true)
        })();
        let cleanup = self.git.must(&["reset", "--quiet", "--hard"]).and_then(|_| self.git.must(&["clean", "-fdq"]));
        match attempt {
            Ok(v) => cleanup.map(|_| v),
            Err(Fail::Conflict(_)) => cleanup.map(|_| false),
            Err(Fail::Error(err)) => Err(err),
        }
    }

    /// A lone unit whose composed tree IS the tree its green checks ran on.
    fn proven(&self, input: &[TrainPipelineUnit], units: &[TrainUnitOutcome], head: &str) -> Result<bool> {
        let [unit] = input else { return Ok(false) };
        let Some(tested) = unit.tested_sha.as_deref() else { return Ok(false) };
        if units[0].outcome != UnitOutcome::Composed {
            return Ok(false);
        }
        let t = self.git.run(&["rev-parse", &format!("{tested}^{{tree}}")])?;
        Ok(t.ok && t.out.trim() == self.git.must(&["rev-parse", &format!("{head}^{{tree}}")])?)
    }

    /// Squash one head onto the tip, resolving generated files.
    fn squash(&mut self, head: &str) -> Step<(Vec<String>, Vec<String>)> {
        let git = &self.git;
        let tip = git.must(&["rev-parse", "HEAD"])?;
        let merge = git.command(&["merge", "--squash", "--no-commit", head]).output()?;
        let conflicted = git.lines(&["diff", "--name-only", "--diff-filter=U"])?;
        if !merge.success && conflicted.is_empty() {
            let detail = if merge.stderr.trim().is_empty() { merge.stdout } else { merge.stderr };
            return Err(Fail::Error(Error::git(format!("git merge --squash {head}"), detail.trim())));
        }
        // A regenerate/renumber rule whose paths BOTH the train and this unit
        // changed is stale even when git merged it cleanly: it runs too.
        let merge_base = git.must(&["merge-base", &tip, head])?;
        let our_changes = git.lines(&["diff", "--name-only", &merge_base, &tip])?;
        let their_changes = git.lines(&["diff", "--name-only", &merge_base, head])?;
        let generated = self.config.generated.clone();
        let rule = |p: &str| rule_of(p, &generated);
        let stale: Vec<String> = their_changes
            .iter()
            .filter(|p| {
                let Some(r) = rule(p) else { return false };
                !conflicted.contains(p)
                    && matches!(generated[r].strategy, ResolveStrategy::Regenerate | ResolveStrategy::Renumber)
                    && our_changes.iter().any(|q| rule(q) == Some(r))
            })
            .cloned()
            .collect();
        if conflicted.is_empty() && stale.is_empty() {
            return Ok((vec![], self.lock_drift(head, &our_changes, &their_changes)?));
        }

        let all: Vec<String> = conflicted.iter().chain(&stale).cloned().collect();
        let steps = match resolve_conflicts(&all, &generated) {
            ResolveVerdict::Conflict(files) => return Err(Fail::Conflict(files)),
            ResolveVerdict::Resolved(steps) => steps,
        };

        // 1. Every rule's file picks, so a later command sees no conflict markers.
        let mut verify: Vec<(Vec<String>, Vec<String>, Vec<String>)> = vec![];
        for step in &steps {
            let rule = generated.iter().find(|r| r.name == step.name).expect("a step names its rule");
            if step.strategy == ResolveStrategy::Renumber {
                let added: Vec<String> = git
                    .lines(&["diff", "--name-only", "--diff-filter=A", &merge_base, head])?
                    .into_iter()
                    .filter(|p| file_number(p).is_some() && rule.paths.iter().any(|g| train_glob_match(g, p)))
                    .collect();
                let globs = rule.verify.clone().unwrap_or_default();
                let mut dropped = vec![];
                for p in added.iter().filter(|p| globs.iter().any(|g| train_glob_match(g, p))) {
                    dropped.push(git.show(head, p)?.unwrap_or_default());
                }
                // The unit's own numbered files go (an add/add keeps the
                // tip's); the rest of the rule takes the tip's side; the
                // command renumbers.
                for p in &added {
                    if conflicted.contains(p) { pick(git, p, Side::Ours, &tip)? } else { remove(git, p)? }
                }
                for p in step.paths.iter().filter(|p| !added.contains(p)) {
                    pick(git, p, Side::Ours, &tip)?;
                }
                verify.push((uniq(step.paths.iter().chain(&added).cloned()), globs, dropped));
            } else {
                for p in &step.paths {
                    if conflicted.contains(p) {
                        pick_by(git, p, step.strategy, &tip)?;
                    } else if step.strategy == ResolveStrategy::Regenerate {
                        pick(git, p, Side::Ours, &tip)?;
                    }
                }
            }
        }
        // 2. The commands, in policy order, after the repo's setup.
        for step in &steps {
            let Some(command) = &step.command else { continue };
            if !self.setup() || !(self.run)(command, &self.git.dir) {
                return Err(Fail::Conflict(step.paths.clone()));
            }
        }
        let git = &self.git;
        git.must(&["add", "-A"])?;
        // 3. A renumber must regenerate exactly what it dropped.
        for (paths, globs, dropped) in &verify {
            if globs.is_empty() {
                continue;
            }
            let mut fresh = vec![];
            for p in git.lines(&["diff", "--cached", "--name-only", "--diff-filter=A", &tip])? {
                if globs.iter().any(|g| train_glob_match(g, &p)) {
                    fresh.push(git.show(":0", &p)?.unwrap_or_default());
                }
            }
            if !same_contents(dropped, &fresh) {
                return Err(Fail::Conflict(paths.clone()));
            }
        }
        // 4. A regenerate must keep the unit's change (#FEED-212): a claimed
        //    path the unit changed that came out as the tip's blob, while the
        //    head's differs from the tip's, was silently dropped.
        let mut lost = vec![];
        for p in steps.iter().filter(|s| s.strategy == ResolveStrategy::Regenerate).flat_map(|s| &s.paths) {
            if !their_changes.contains(p) {
                continue;
            }
            let tip_blob = git.blob_at(&tip, p)?;
            if git.blob_at("", p)? == tip_blob && git.blob_at(head, p)? != tip_blob {
                lost.push(p.clone());
            }
        }
        if !lost.is_empty() {
            return Err(Fail::Conflict(lost));
        }
        let left = git.lines(&["diff", "--name-only", "--diff-filter=U"])?;
        if !left.is_empty() {
            return Err(Fail::Conflict(left));
        }
        Ok((steps.into_iter().map(|s| s.name).collect(), self.lock_drift(head, &our_changes, &their_changes)?))
    }

    /// #LOGS-2 (`repoPolicy.lockfiles`): the unit's lockfiles whose composed
    /// blob is not the one at its head, + the paired locks the train moved
    /// under a manifest the unit changed.
    fn lock_drift(&self, head: &str, ours: &[String], theirs: &[String]) -> Result<Vec<String>> {
        let Some(policy) = &self.config.lockfiles else { return Ok(vec![]) };
        let is_lock = |p: &str| policy.paths.iter().any(|g| train_glob_match(g, p));
        let mut drifted = vec![];
        for p in theirs.iter().filter(|p| is_lock(p)) {
            if self.git.blob_at("", p)? != self.git.blob_at(head, p)? {
                drifted.push(p.clone());
            }
        }
        for pair in policy.manifests.iter().flatten() {
            let changed: Vec<&String> = theirs.iter().filter(|p| pair.paths.iter().any(|g| train_glob_match(g, p))).collect();
            for lock in ours {
                if lock.rsplit('/').next() != Some(pair.lock.as_str()) || !is_lock(lock) || drifted.contains(lock) {
                    continue;
                }
                let root = lock.rfind('/').map_or("", |i| &lock[..i]);
                if changed.iter().any(|m| root.is_empty() || m.starts_with(&format!("{root}/"))) {
                    drifted.push(lock.clone());
                }
            }
        }
        Ok(drifted)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Ours,
    Theirs,
}

fn remove(git: &Git, path: &str) -> Result<()> {
    git.run(&["rm", "--quiet", "--force", "--cached", "--", path])?;
    let _ = std::fs::remove_file(git.dir.join(path));
    Ok(())
}

/// Take one side's version (its deletion included); a path git merged
/// cleanly takes the train tip's.
fn pick(git: &Git, path: &str, side: Side, tip: &str) -> Result<()> {
    let unmerged = !git.lines(&["ls-files", "--unmerged", "--", path])?.is_empty();
    if !unmerged {
        if git.show(tip, path)?.is_none() {
            return remove(git, path);
        }
        git.must(&["checkout", tip, "--", path])?;
        return Ok(());
    }
    if git.stage(if side == Side::Ours { 2 } else { 3 }, path)?.is_none() {
        return remove(git, path);
    }
    git.must(&["checkout", if side == Side::Ours { "--ours" } else { "--theirs" }, "--", path])?;
    git.must(&["add", "--", path])?;
    Ok(())
}

fn pick_by(git: &Git, path: &str, strategy: ResolveStrategy, tip: &str) -> Step<()> {
    match strategy {
        ResolveStrategy::Regenerate | ResolveStrategy::Renumber => return Ok(pick(git, path, Side::Ours, tip)?),
        ResolveStrategy::TakeLater => return Ok(pick(git, path, Side::Theirs, tip)?),
        _ => {}
    }
    let (base, ours, theirs) = (git.stage(1, path)?, git.stage(2, path)?, git.stage(3, path)?);
    let (ours, theirs) = match (ours, theirs) {
        (Some(ours), Some(theirs)) => (ours, theirs),
        (_, theirs) => return Ok(pick(git, path, if theirs.is_none() { Side::Ours } else { Side::Theirs }, tip)?),
    };
    let text = if strategy == ResolveStrategy::JsonUnion {
        json_union(base.as_deref(), &ours, &theirs).map_err(|e| Fail::Error(Error::Invalid(format!("{path}: {e}"))))?
    } else {
        // prepend-union: each side ONE pure insertion at one split point,
        // the later unit's block first; anything else bounces (#LOGS-2).
        prepend_union(base.as_deref().unwrap_or(""), &ours, &theirs).ok_or_else(|| Fail::Conflict(vec![path.to_string()]))?
    };
    std::fs::write(git.dir.join(path), text).map_err(|e| Error::Invalid(format!("write {path}: {e}")))?;
    git.must(&["add", "--", path])?;
    Ok(())
}
