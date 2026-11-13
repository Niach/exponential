//! The `local` provider: a [`Forge`] with no server. Branches are the
//! [`LocalRepo`]'s refs (each moved by compare-and-swap); PRs and events
//! are one table object in a [`Store`], swapped by `update_ref` (the ONE
//! serialization point: a writer that lost re-reads and retries). Every op
//! follows `forge.json`; `tests/forge_replay.rs` replays it on a real repo.
//!
//! Ordering: the git refs move first, the table swaps last. A table swap
//! that loses after the refs moved returns `Store(Conflict)` with the refs
//! already moved; one writer per repository (a device's runner) avoids it.

use ecode_core::{diff_trees, supports, Capability, ForgeReason, Mergeability, PrState, Provider, PullRequest};
use serde::{Deserialize, Serialize};

use crate::repo::classify;
use crate::{
    Error, Event, Events, Expected, FileChange, Forge, LocalRepo, MergeCheck, ObjectKind, OpenPr, PushSource, Result,
    Store, StoreExt,
};

/// The table a local forge keeps in its store.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Table {
    prs: Vec<PullRequest>,
    next_pr: i64,
    events: Vec<Event>,
}

impl Default for Table {
    fn default() -> Self {
        Self { prs: vec![], next_pr: 1, events: vec![] }
    }
}

/// A forge over a [`LocalRepo`] and a [`Store`].
pub struct LocalForge<S> {
    repo: LocalRepo,
    store: S,
    /// The store ref naming the table.
    table_ref: String,
    default_branch: String,
}

fn refuse<T>(reason: ForgeReason) -> Result<T> {
    Err(Error::refused(reason))
}

impl<S: Store> LocalForge<S> {
    /// A forge on `repo` whose table lives in `store` under `forge/table`.
    pub fn new(repo: LocalRepo, store: S, default_branch: impl Into<String>) -> Self {
        Self { repo, store, table_ref: "forge/table".into(), default_branch: default_branch.into() }
    }

    pub fn repo(&self) -> &LocalRepo {
        &self.repo
    }

    pub fn store(&self) -> &S {
        &self.store
    }

    pub fn default_branch(&self) -> &str {
        &self.default_branch
    }

    /// Adds existing PRs to the table (a migration, a test's given state);
    /// numbering continues after the highest.
    pub fn import(&self, prs: Vec<PullRequest>) -> Result<()> {
        self.transact(|table| {
            let highest = prs.iter().map(|pr| pr.number).fold(table.next_pr - 1, i64::max);
            table.next_pr = highest + 1;
            table.prs.extend(prs);
            Ok(())
        })
    }

    /// Every PR, by number.
    pub fn prs(&self) -> Result<Vec<PullRequest>> {
        Ok(self.load()?.1.prs)
    }

    fn load(&self) -> Result<(Option<String>, Table)> {
        let version = self.store.read_ref(&self.table_ref)?;
        let table = match &version {
            Some(id) => self
                .store
                .get_json::<Table>(ObjectKind::Review, id)?
                .ok_or_else(|| Error::Invalid(format!("table object {id} is missing")))?,
            None => Table::default(),
        };
        Ok((version, table))
    }

    fn save(&self, version: Option<String>, table: &Table) -> Result<()> {
        let id = self.store.put_json(ObjectKind::Review, table)?;
        Ok(self.store.update_ref(&self.table_ref, version.as_deref(), Some(&id))?)
    }

    /// Runs `op` on the table and saves it when it emitted anything.
    fn transact<T>(&self, op: impl FnOnce(&mut Table) -> Result<T>) -> Result<T> {
        let (version, mut table) = self.load()?;
        let events = table.events.len();
        let prs = table.prs.clone();
        let out = op(&mut table)?;
        if table.events.len() != events || table.prs != prs {
            self.save(version, &table)?;
        }
        Ok(out)
    }

    fn tip(&self, branch: &str, reason: ForgeReason) -> Result<String> {
        match self.repo.branch(branch)? {
            Some(tip) => Ok(tip),
            None => refuse(reason),
        }
    }

    fn live(table: &Table, number: i64) -> Result<usize> {
        let Some(index) = table.prs.iter().position(|pr| pr.number == number) else { return refuse(ForgeReason::Pr) };
        if !table.prs[index].state.is_live() {
            return refuse(ForgeReason::NotOpen);
        }
        Ok(index)
    }

    fn check(&self, pr: &PullRequest) -> Result<(MergeCheck, String, String)> {
        let head = self.tip(&pr.head, ForgeReason::Head)?;
        let base = self.tip(&pr.base, ForgeReason::Base)?;
        let (check, _) = self.repo.mergeability(&base, &head)?;
        Ok((check, base, head))
    }

    fn squash(&self, table: &mut Table, number: i64, expected_head: Option<&str>) -> Result<String> {
        let index = Self::live(table, number)?;
        let pr = table.prs[index].clone();
        if pr.state == PrState::Draft {
            return refuse(ForgeReason::Draft);
        }
        let head = self.tip(&pr.head, ForgeReason::Head)?;
        if expected_head.is_some_and(|expected| expected != head) {
            return refuse(ForgeReason::HeadMoved);
        }
        let (check, base, head) = self.check(&pr)?;
        if check.mergeability == Mergeability::Dirty {
            return refuse(ForgeReason::Dirty);
        }
        let title = if pr.title.is_empty() { pr.head.clone() } else { pr.title.clone() };
        let sha = self.repo.squash_commit(&base, &head, &format!("{title} (#{number})\n"))?;
        self.repo.set_branch(&pr.base, &sha, Expected::Is(&base))?;
        let merged = &mut table.prs[index];
        merged.state = PrState::Merged;
        merged.head_sha = Some(head.clone());
        merged.merged_sha = Some(sha.clone());
        table.events.push(Event::PrMerged { number, sha: sha.clone() });
        // The PR tree: every live child moves onto the merged PR's base (EXP-324).
        for child in table.prs.iter_mut() {
            if child.state.is_live() && child.base == pr.head {
                child.base = pr.base.clone();
                table.events.push(Event::PrRetargeted { number: child.number, from: pr.head.clone(), to: pr.base.clone() });
            }
        }
        if pr.head != self.default_branch && !table.prs.iter().any(|o| o.state.is_live() && o.head == pr.head) {
            self.repo.delete_branch(&pr.head, Expected::Is(&head))?;
            table.events.push(Event::BranchDeleted { branch: pr.head.clone() });
        }
        Ok(sha)
    }
}

impl<S: Store> Forge for LocalForge<S> {
    fn provider(&self) -> Provider {
        Provider::Ecode
    }

    fn branch(&self, name: &str) -> Result<Option<String>> {
        self.repo.branch(name)
    }

    fn pr(&self, number: i64) -> Result<Option<PullRequest>> {
        Ok(self.load()?.1.prs.into_iter().find(|pr| pr.number == number))
    }

    fn push(&self, branch: &str, source: PushSource, force: bool) -> Result<String> {
        self.transact(|table| {
            let current = self.repo.branch(branch)?;
            let target = match source {
                PushSource::Sha(sha) => {
                    let Some(target) = self.repo.resolve(&sha)?.filter(|t| *t == sha) else {
                        return refuse(ForgeReason::Commit);
                    };
                    if let Some(current) = &current {
                        if !force && !self.repo.is_ancestor(current, &target)? {
                            return refuse(ForgeReason::NonFastForward);
                        }
                    }
                    target
                }
                PushSource::Commit { parent, changes, message } => {
                    let parent = match parent {
                        Some(p) => match self.repo.resolve(&p)?.filter(|t| *t == p) {
                            Some(p) => Some(p),
                            None => return refuse(ForgeReason::Parent),
                        },
                        None => None,
                    };
                    let fast_forward = match (&current, &parent) {
                        (None, _) => true,
                        (Some(current), Some(parent)) => self.repo.is_ancestor(current, parent)?,
                        (Some(_), None) => false,
                    };
                    if !fast_forward && !force {
                        return refuse(ForgeReason::NonFastForward);
                    }
                    self.repo.commit(parent.as_deref(), &changes, &message)?
                }
            };
            self.repo.set_branch(branch, &target, current.as_deref().into())?;
            table.events.push(Event::Pushed { branch: branch.into(), sha: target.clone(), forced: false });
            Ok(target)
        })
    }

    fn open(&self, pr: OpenPr) -> Result<i64> {
        self.transact(|table| {
            self.tip(&pr.head, ForgeReason::Head)?;
            self.tip(&pr.base, ForgeReason::Base)?;
            if pr.head == pr.base {
                return refuse(ForgeReason::SameBranch);
            }
            if table.prs.iter().any(|o| o.state.is_live() && o.head == pr.head) {
                return refuse(ForgeReason::Exists);
            }
            if crate::closes_loop(&table.prs, &pr.head, &pr.base) {
                return refuse(ForgeReason::Cycle);
            }
            let number = table.next_pr;
            table.next_pr += 1;
            table.prs.push(PullRequest {
                number,
                head: pr.head,
                base: pr.base,
                title: pr.title,
                state: if pr.draft { PrState::Draft } else { PrState::Open },
                head_sha: None,
                merged_sha: None,
            });
            table.events.push(Event::PrOpened { number });
            Ok(number)
        })
    }

    fn list_by_base(&self, base: &str) -> Result<Vec<i64>> {
        let mut numbers: Vec<i64> =
            self.prs()?.iter().filter(|pr| pr.state.is_live() && pr.base == base).map(|pr| pr.number).collect();
        numbers.sort_unstable();
        Ok(numbers)
    }

    fn retarget(&self, number: i64, base: &str) -> Result<()> {
        self.transact(|table| {
            let index = Self::live(table, number)?;
            self.tip(base, ForgeReason::Base)?;
            let pr = &table.prs[index];
            if base == pr.head {
                return refuse(ForgeReason::SameBranch);
            }
            if !supports(Provider::Ecode, Capability::Retarget) {
                return refuse(ForgeReason::Unsupported);
            }
            if base == pr.base {
                return Ok(());
            }
            if crate::closes_loop(&table.prs, &pr.head, base) {
                return refuse(ForgeReason::Cycle);
            }
            table.events.push(Event::PrRetargeted { number, from: pr.base.clone(), to: base.into() });
            table.prs[index].base = base.into();
            Ok(())
        })
    }

    fn mergeability(&self, number: i64) -> Result<MergeCheck> {
        let (_, table) = self.load()?;
        let index = Self::live(&table, number)?;
        Ok(self.check(&table.prs[index])?.0)
    }

    fn squash_merge(&self, number: i64, expected_head: Option<&str>) -> Result<String> {
        self.transact(|table| self.squash(table, number, expected_head))
    }

    fn close(&self, number: i64) -> Result<()> {
        self.transact(|table| {
            let index = Self::live(table, number)?;
            let head_sha = self.repo.branch(&table.prs[index].head)?;
            let pr = &mut table.prs[index];
            pr.state = PrState::Closed;
            pr.head_sha = head_sha;
            table.events.push(Event::PrClosed { number });
            Ok(())
        })
    }

    fn files(&self, number: i64) -> Result<Vec<FileChange>> {
        let (_, table) = self.load()?;
        let pr = &table.prs[Self::live(&table, number)?];
        let head = self.tip(&pr.head, ForgeReason::Head)?;
        let base = self.tip(&pr.base, ForgeReason::Base)?;
        let before = self.repo.tree(self.repo.merge_base(&head, &base)?.as_deref())?;
        let after = self.repo.tree(Some(&head))?;
        Ok(classify(&before, &diff_trees(&before, &after)))
    }

    fn rebase_onto(&self, branch: &str, onto: &str, upstream: &str) -> Result<String> {
        self.transact(|table| {
            let from = self.tip(branch, ForgeReason::Head)?;
            let onto_tip = self.tip(onto, ForgeReason::Base)?;
            // The commits after `upstream`, oldest first (first parents).
            let Some(upstream) = self.repo.resolve(upstream)? else { return refuse(ForgeReason::Upstream) };
            let mut chain = vec![];
            let mut cursor = Some(from.clone());
            loop {
                match cursor {
                    Some(current) if current == upstream => break,
                    Some(current) => {
                        let entry = self.repo.log(&current, 1)?.pop().ok_or_else(|| Error::Invalid(current.clone()))?;
                        cursor = entry.parents.first().cloned();
                        chain.push(entry);
                    }
                    // A root reached: `upstream` is not in the branch's history.
                    None => return refuse(ForgeReason::Upstream),
                }
            }
            // The spec's path-wise check first (`forge.json`): a commit whose
            // path moved on `onto` to anything but its result is dirty. git
            // would merge some of those line-wise; the spec does not.
            let mut tree = self.repo.tree(Some(&onto_tip))?;
            for entry in chain.iter().rev() {
                let before = self.repo.tree(entry.parents.first().map(String::as_str))?;
                let changes = diff_trees(&before, &self.repo.tree(Some(&entry.sha))?);
                let mut next = tree.clone();
                for (path, content) in &changes {
                    let (was, now) = (before.get(path), tree.get(path));
                    if now != was && now != content.as_ref() {
                        return refuse(ForgeReason::Dirty);
                    }
                    match content {
                        Some(content) => next.insert(path.clone(), content.clone()),
                        None => next.remove(path),
                    };
                }
                tree = next;
            }
            let sha = self.repo.rebase_commits(&from, &onto_tip, &upstream)?;
            self.repo.set_branch(branch, &sha, Expected::Is(&from))?;
            table.events.push(Event::Pushed { branch: branch.into(), sha: sha.clone(), forced: true });
            Ok(sha)
        })
    }

    fn events(&self, cursor: Option<&str>) -> Result<Events> {
        let (_, table) = self.load()?;
        let from = cursor.and_then(|c| c.parse::<usize>().ok()).unwrap_or(0).min(table.events.len());
        Ok(Events { events: table.events[from..].to_vec(), cursor: table.events.len().to_string() })
    }
}
