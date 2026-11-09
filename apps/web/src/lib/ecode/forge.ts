// EXP-1709 (#EXP-1614, merge-train.json): THE FORGE SEAM. Everything the
// merge train (and the live-PR backfill) asks of a code host goes through
// this interface, provider-neutral: no GitHub type crosses it. GitHub is the
// one implementation today (`forge-github.ts`); eCode's own forge plugs in
// here next, behind `forgeFor`.
import type {
  TrainCheck,
  TrainPipelineInputs,
  TrainPipelineOutcome,
} from "@exp/domain-contract/merge-train"

/** The repository a forge call is about (a `repositories` row). */
export interface ForgeRepo {
  id: string
  /** `owner/name` as the forge names it. */
  fullName: string
  installationId?: number | null
}

/** What a forge says about one PR. */
export interface ForgePullRequest {
  number: number
  state: `open` | `closed`
  merged: boolean
  draft: boolean
  headSha: string | null
  headRef: string | null
  baseRef: string | null
  title: string
  mergedAt: Date | null
  /** The forge account that merged it (raw, resolved by the caller). */
  mergedBy: { id?: number | null; login?: string | null; type?: string | null } | null
  /** The PR author as a commit author line `Name <email>`, when known. */
  author: string | null
}

/** One pipeline run's result as the forge holds it. */
export interface ForgePipelineResult {
  /** The pipeline's report (merge-train.json `pipeline.check`), null = none yet. */
  outcome: TrainPipelineOutcome | null
  /** The run finished (with or without a report); false = unknown or running. */
  finished: boolean
}

export type FastForwardResult =
  | { ok: true }
  /** The branch is no longer at the expected sha (someone landed): recompose. */
  | { ok: false; reason: `moved`; current: string | null }
  /** The forge refused (permissions, a ruleset without the App on its bypass list). */
  | { ok: false; reason: `refused`; message: string }

export type DirectMergeResult = { ok: true; sha: string | null } | { ok: false; conflict: boolean; message: string }

export interface Forge {
  readonly kind: string
  readPullRequest(repo: ForgeRepo, number: number): Promise<ForgePullRequest | null>
  /** The sha a branch points at, null when it does not exist. */
  readBranch(repo: ForgeRepo, branch: string): Promise<string | null>
  /** The base branch's REQUIRED check names (never hard-coded). */
  requiredChecks(repo: ForgeRepo, base: string): Promise<string[]>
  /** Start ONE pipeline run (merge-train.json `pipeline.inputs`). `missing` =
   *  the repo has no pipeline installed (the train then lands directly). */
  dispatchPipeline(
    repo: ForgeRepo,
    inputs: TrainPipelineInputs
  ): Promise<{ ok: true; runRef: string | null } | { ok: false; missing: boolean; message: string }>
  /** The run's report (the `exp-train` result on `exp-train/<train_id>`). */
  readPipeline(repo: ForgeRepo, trainId: string, runRef: string | null): Promise<ForgePipelineResult>
  /** Every check at `sha`, null = unreadable (no permission, rate limited). */
  checksAt(repo: ForgeRepo, sha: string): Promise<TrainCheck[] | null>
  /** #FEED-212: `branch` points at or contains `sha` (read now, after a
   *  push); null = unreadable. A landing is confirmed only through this. */
  branchContains(repo: ForgeRepo, branch: string, sha: string): Promise<boolean | null>
  /** Move `branch` from `from` to `to`, fast-forward only. */
  fastForward(repo: ForgeRepo, args: { branch: string; from: string; to: string }): Promise<FastForwardResult>
  /** Close a PR whose commits landed through the train, with a comment
   *  naming the commit (the forge shows Closed; Exponential shows Merged). */
  closeAsLanded(repo: ForgeRepo, args: { number: number; sha: string; base: string }): Promise<void>
  /** A repo without the pipeline: the forge's own squash merge, one PR at a
   *  time (serialized, untested, unresolved). `expectedHead` guards a push. */
  squashMerge(
    repo: ForgeRepo,
    args: { number: number; title: string; expectedHead: string | null }
  ): Promise<DirectMergeResult>
}

let override: ((repo: ForgeRepo) => Forge | null) | null = null

/** The forge of a repository. Every repository is a GitHub one today. */
export async function forgeFor(repo: ForgeRepo): Promise<Forge> {
  const forged = override?.(repo)
  if (forged) return forged
  const { githubForge } = await import(`@/lib/ecode/forge-github`)
  return githubForge
}

/** Tests: route every forgeFor through `fake` (null restores the default). */
export function setForgeForTests(fake: ((repo: ForgeRepo) => Forge | null) | null): void {
  override = fake
}
