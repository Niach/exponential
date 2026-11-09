// EXP-1708 (fixtures/merge-train.json `pipeline`): THE TRAIN ENGINE. Generic:
// it knows git and the repo's `.exponential/merge-train.json`, nothing about
// this repo and nothing about a forge. From `base_sha`, each PR of each unit
// in queue order is squash-merged onto the train tip; conflicted paths go
// through the repo's resolver rules; a unit that cannot be resolved is dropped
// whole (the tip resets to before it) and composing goes on.

import { rmSync, writeFileSync } from "node:fs"
import { join } from "node:path"
import {
  globMatch,
  resolveConflicts,
  trainChecksMode,
  type ResolveStrategy,
  type TrainChecksMode,
  type TrainPipelineOutcome,
  type TrainPipelineUnit,
  type TrainRepoConfig,
  type TrainUnitOutcome,
} from "../../packages/domain-contract/src/merge-train"
import { Git, sh } from "./git"
import { fileNumber, jsonUnion, prependUnion, sameContents } from "./strategies"

export interface ComposeInput {
  /** A clone with base_sha and every head (and tested_sha) fetched. */
  dir: string
  trainId: string
  baseSha: string
  mode: `test` | `untested`
  units: TrainPipelineUnit[]
  /** null = the base's own `.exponential/merge-train.json`. */
  config?: TrainRepoConfig | null
  /** Runs a repo command (setup, a rule's command) in `dir`. */
  runCommand?: (command: string, dir: string) => boolean
}

export interface ComposeResult {
  outcome: TrainPipelineOutcome
  /** The CI workflows the caller starts on the ref (`checks` = dispatched). */
  dispatch: string[]
}

const shell = (command: string, dir: string): boolean => {
  const r = sh(dir, [`bash`, `-euo`, `pipefail`, `-c`, command])
  if (!r.ok) console.error(`[merge-train] \`${command}\` failed:\n${r.err || r.out}`)
  return r.ok
}

/** The repo's policy as the BASE declares it (never a unit's). */
export function readConfig(git: Git, baseSha: string): TrainRepoConfig {
  const text = git.show(baseSha, `.exponential/merge-train.json`)
  if (!text) return { generated: [] }
  const parsed = JSON.parse(text) as Partial<TrainRepoConfig>
  return { setup: parsed.setup, ci: parsed.ci, generated: parsed.generated ?? [], ...(parsed.lockfiles ? { lockfiles: parsed.lockfiles } : {}) }
}

class UnitConflict extends Error {
  constructor(readonly files: string[]) {
    super(`conflict`)
  }
}

export function compose(input: ComposeInput): ComposeResult {
  const git = new Git(input.dir)
  const run = input.runCommand ?? shell
  const config = input.config ?? readConfig(git, input.baseSha)
  let setupDone = false
  git.must(`checkout`, `--quiet`, `--detach`, input.baseSha)
  git.must(`reset`, `--quiet`, `--hard`, input.baseSha)

  const units: TrainUnitOutcome[] = []
  for (const unit of input.units) {
    const before = git.must(`rev-parse`, `HEAD`)
    const prs = unit.prs.map((p) => p.number)
    const resolved = new Set<string>()
    const drifted = new Set<string>()
    try {
      for (const pr of unit.prs) {
        const squashed = squash(git, pr.head_sha, config, run, () => {
          if (setupDone || !config.setup) return true
          setupDone = true
          return run(config.setup, input.dir)
        })
        for (const name of squashed.resolved) resolved.add(name)
        for (const path of squashed.drifted) drifted.add(path)
        const message = `${pr.title} (#${pr.number})`
        if (git.run(`diff`, `--cached`, `--quiet`).ok) continue // already in the base
        git.must(`commit`, `--quiet`, `--no-verify`, `-m`, message, ...(pr.author ? [`--author`, pr.author] : []))
      }
      units.push({ prs, outcome: `composed`, files: [], resolved: [...resolved], ...(drifted.size > 0 ? { drifted: [...drifted] } : {}) })
    } catch (e) {
      if (!(e instanceof UnitConflict)) throw e
      git.must(`reset`, `--quiet`, `--hard`, before)
      git.must(`clean`, `-fdq`)
      // #EXP-1711: behind earlier units, does it compose on the base alone?
      // Then only a sibling that has not landed is in its way: no bounce.
      const alone = before !== input.baseSha && composesAlone(git, input.baseSha, unit, config, run, () => {
        if (setupDone || !config.setup) return true
        setupDone = true
        return run(config.setup, input.dir)
      })
      git.must(`checkout`, `--quiet`, `--detach`, before)
      git.must(`reset`, `--quiet`, `--hard`, before)
      git.must(`clean`, `-fdq`)
      units.push({ prs, outcome: `conflict`, files: e.files, resolved: [], ...(alone ? { against: `batch` as const } : {}) })
    }
  }

  const composed = units.some((u) => u.outcome === `composed`)
  if (!composed) git.must(`commit`, `--quiet`, `--allow-empty`, `-m`, `exp-train ${input.trainId}: nothing composed`)
  const head = git.must(`rev-parse`, `HEAD`)
  const workflows = config.ci?.workflows ?? []
  // #LOGS-2: a Merge now is untested only when nothing was resolved and no
  // lockfile drifted from its head; else the tree is one nobody tested.
  const riding = units.filter((u) => u.outcome === `composed`)
  const checks: TrainChecksMode = trainChecksMode({
    mode: input.mode,
    composed,
    resolved: riding.some((u) => u.resolved.length > 0),
    drifted: riding.some((u) => (u.drifted ?? []).length > 0),
    proven: composed && proven(git, input.units, units, head),
    workflows,
  })
  return {
    outcome: { train: input.trainId, base_sha: input.baseSha, head, checks, units, error: null },
    dispatch: checks === `dispatched` ? workflows : [],
  }
}

/** #EXP-1711: the unit squashed onto the bare base, thrown away after. */
function composesAlone(
  git: Git,
  baseSha: string,
  unit: TrainPipelineUnit,
  config: TrainRepoConfig,
  run: (command: string, dir: string) => boolean,
  setup: () => boolean
): boolean {
  git.must(`checkout`, `--quiet`, `--detach`, baseSha)
  git.must(`reset`, `--quiet`, `--hard`, baseSha)
  try {
    for (const pr of unit.prs) {
      squash(git, pr.head_sha, config, run, setup)
      if (git.run(`diff`, `--cached`, `--quiet`).ok) continue
      git.must(`commit`, `--quiet`, `--no-verify`, `-m`, `probe #${pr.number}`)
    }
    return true
  } catch (e) {
    if (e instanceof UnitConflict) return false
    throw e
  } finally {
    git.must(`reset`, `--quiet`, `--hard`)
    git.must(`clean`, `-fdq`)
  }
}

/** A lone unit whose composed tree IS the tree its green checks ran on. */
function proven(git: Git, input: TrainPipelineUnit[], units: TrainUnitOutcome[], head: string): boolean {
  if (input.length !== 1 || units[0].outcome !== `composed` || !input[0].tested_sha) return false
  const tested = git.run(`rev-parse`, `${input[0].tested_sha}^{tree}`)
  return tested.ok && tested.out.trim() === git.must(`rev-parse`, `${head}^{tree}`)
}

/** Squash one head onto the tip, resolving generated files; → the rule
 *  names applied + the drifted lockfiles (`lockDrift`). Throws
 *  UnitConflict with the files that bounce it. */
function squash(
  git: Git,
  head: string,
  config: TrainRepoConfig,
  run: (command: string, dir: string) => boolean,
  setup: () => boolean
): { resolved: string[]; drifted: string[] } {
  const tip = git.must(`rev-parse`, `HEAD`)
  const merge = git.run(`merge`, `--squash`, `--no-commit`, head)
  const conflicted = git.lines(`diff`, `--name-only`, `--diff-filter=U`)
  if (!merge.ok && conflicted.length === 0) throw new Error(`git merge --squash ${head}: ${merge.err.trim() || merge.out.trim()}`)
  // A regenerate/renumber rule whose paths BOTH the train and this unit
  // changed is stale even when git merged it cleanly: it runs too.
  const mergeBase = git.must(`merge-base`, tip, head)
  const ourChanges = git.lines(`diff`, `--name-only`, mergeBase, tip)
  const theirChanges = git.lines(`diff`, `--name-only`, mergeBase, head)
  const ruleOf = (p: string) => config.generated.find((r) => r.paths.some((g) => globMatch(g, p)))
  const stale = theirChanges.filter((p) => {
    const rule = ruleOf(p)
    return (
      !conflicted.includes(p) &&
      (rule?.strategy === `regenerate` || rule?.strategy === `renumber`) &&
      ourChanges.some((q) => ruleOf(q) === rule)
    )
  })
  if (conflicted.length === 0 && stale.length === 0) return { resolved: [], drifted: lockDrift(git, head, config, ourChanges, theirChanges) }

  const verdict = resolveConflicts([...conflicted, ...stale], config.generated)
  if (!verdict.ok) throw new UnitConflict(verdict.files)

  // 1. Every rule's file picks, so a later command sees no conflict markers.
  const verify: Array<{ paths: string[]; globs: string[]; dropped: string[] }> = []
  for (const step of verdict.steps) {
    const rule = config.generated.find((r) => r.name === step.name)!
    if (step.strategy === `renumber`) {
      const added = git
        .lines(`diff`, `--name-only`, `--diff-filter=A`, mergeBase, head)
        .filter((p) => fileNumber(p) !== null && rule.paths.some((g) => globMatch(g, p)))
      const globs = rule.verify ?? []
      const dropped = added.filter((p) => globs.some((g) => globMatch(g, p))).map((p) => git.show(head, p) ?? ``)
      // The unit's own numbered files go (an add/add keeps the tip's); the
      // rest of the rule takes the tip's side; the command renumbers.
      for (const p of added) conflicted.includes(p) ? pick(git, p, `ours`, tip) : remove(git, p)
      for (const p of step.paths) if (!added.includes(p)) pick(git, p, `ours`, tip)
      verify.push({ paths: [...new Set([...step.paths, ...added])], globs, dropped })
    } else {
      for (const p of step.paths) {
        if (conflicted.includes(p)) pickBy(git, p, step.strategy, tip)
        else if (step.strategy === `regenerate`) pick(git, p, `ours`, tip)
      }
    }
  }
  // 2. The commands, in policy order, after the repo's setup.
  for (const step of verdict.steps) {
    if (!step.command) continue
    if (!setup() || !run(step.command, git.dir)) throw new UnitConflict(step.paths)
  }
  git.must(`add`, `-A`)
  // 3. A renumber must regenerate exactly what it dropped.
  for (const v of verify) {
    if (v.globs.length === 0) continue
    const fresh = git
      .lines(`diff`, `--cached`, `--name-only`, `--diff-filter=A`, tip)
      .filter((p) => v.globs.some((g) => globMatch(g, p)))
      .map((p) => git.show(`:0`, p) ?? ``)
    if (!sameContents(v.dropped, fresh)) throw new UnitConflict(v.paths)
  }
  // 4. A regenerate must keep the unit's change (#FEED-212): a claimed path
  //    the unit changed that came out as the tip's blob, while the head's
  //    differs from the tip's, was silently dropped by a command that never
  //    regenerated it.
  const dropped = verdict.steps
    .filter((s) => s.strategy === `regenerate`)
    .flatMap((s) => s.paths)
    .filter((p) => {
      if (!theirChanges.includes(p)) return false
      const tipBlob = blobAt(git, tip, p)
      return blobAt(git, ``, p) === tipBlob && blobAt(git, head, p) !== tipBlob
    })
  if (dropped.length > 0) throw new UnitConflict(dropped)
  const left = git.lines(`diff`, `--name-only`, `--diff-filter=U`)
  if (left.length > 0) throw new UnitConflict(left)
  return { resolved: verdict.steps.map((s) => s.name), drifted: lockDrift(git, head, config, ourChanges, theirChanges) }
}

/** #LOGS-2 (`repoPolicy.lockfiles`): the unit's lockfiles whose composed blob
 *  is not the one at its head (git merged a lock cleanly onto a base that
 *  moved it: a lock nobody resolved), + the paired locks the train moved
 *  under a manifest the unit changed. */
function lockDrift(git: Git, head: string, config: TrainRepoConfig, ourChanges: string[], theirChanges: string[]): string[] {
  const policy = config.lockfiles
  if (!policy) return []
  const isLock = (p: string) => policy.paths.some((g) => globMatch(g, p))
  const blob = (rev: string, p: string) => blobAt(git, rev, p)
  const drifted = theirChanges.filter((p) => isLock(p) && blob(``, p) !== blob(head, p))
  const dirOf = (p: string) => (p.includes(`/`) ? p.slice(0, p.lastIndexOf(`/`)) : ``)
  for (const pair of policy.manifests ?? []) {
    const changed = theirChanges.filter((p) => pair.paths.some((g) => globMatch(g, p)))
    for (const lock of ourChanges) {
      if (lock.split(`/`).pop() !== pair.lock || !isLock(lock) || drifted.includes(lock)) continue
      const root = dirOf(lock)
      if (changed.some((m) => root === `` || m.startsWith(`${root}/`))) drifted.push(lock)
    }
  }
  return drifted
}

/** A path's blob id at `rev` (`` = the index); null = absent. */
function blobAt(git: Git, rev: string, path: string): string | null {
  const r = git.run(`rev-parse`, `--verify`, `--quiet`, `${rev}:${path}`)
  return r.ok ? r.out.trim() : null
}

function remove(git: Git, path: string): void {
  git.run(`rm`, `--quiet`, `--force`, `--cached`, `--`, path)
  rmSync(join(git.dir, path), { force: true })
}

/** Take one side's version (its deletion included); a path git merged
 *  cleanly takes the train tip's. */
function pick(git: Git, path: string, side: `ours` | `theirs`, tip: string): void {
  const unmerged = git.lines(`ls-files`, `--unmerged`, `--`, path).length > 0
  if (!unmerged) {
    if (git.show(tip, path) === null) return remove(git, path)
    git.must(`checkout`, tip, `--`, path)
    return
  }
  if (git.stage(side === `ours` ? 2 : 3, path) === null) return remove(git, path)
  git.must(`checkout`, `--${side}`, `--`, path)
  git.must(`add`, `--`, path)
}

function pickBy(git: Git, path: string, strategy: ResolveStrategy, tip: string): void {
  if (strategy === `regenerate`) return pick(git, path, `ours`, tip)
  if (strategy === `take-later`) return pick(git, path, `theirs`, tip)
  const base = git.stage(1, path)
  const ours = git.stage(2, path)
  const theirs = git.stage(3, path)
  if (ours === null || theirs === null) return pick(git, path, theirs === null ? `ours` : `theirs`, tip)
  let text: string
  if (strategy === `json-union`) text = jsonUnion(base, ours, theirs)
  else {
    // prepend-union: each side ONE pure insertion at one split point, the
    // later unit's block first; anything else bounces (#LOGS-2).
    const merged = prependUnion(base ?? ``, ours, theirs)
    if (merged === null) throw new UnitConflict([path])
    text = merged
  }
  writeFileSync(join(git.dir, path), text)
  git.must(`add`, `--`, path)
}
