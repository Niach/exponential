// EXP-1709 (#EXP-1614, merge-train.json): THE FORGE SEAM. Everything the
// server asks of a code host goes through this interface, provider-neutral:
// no GitHub type crosses it. The merge train, the PR model
// (`lib/pull-requests.ts`) and the PR state writer (`pr-sync.ts`) call ONLY
// this (forge-seam.test.ts greps it). The ops are eCode's PR-tree primitives
// (packages/ecode `fixtures/forge.json`, every implementation conforms:
// forge-conformance.test.ts). GitHub is the one implementation today
// (`forge-github.ts`) and eCode's own (`forge-ecode.ts`, #EXP-1616).
import type {
  TrainCheck,
  TrainPipelineInputs,
  TrainPipelineOutcome,
} from "@exp/domain-contract/merge-train"
import type { Capability, ForgeReason, Mergeability, Provider } from "@exp/ecode"

/** The repository a forge call is about (a `repositories` row). */
export interface ForgeRepo {
  id: string
  /** `owner/name` as the forge names it. */
  fullName: string
  /** `repositories.provider`; absent = github (every pre-eCode row). */
  provider?: Provider
  /** GitHub's App installation (`provider_config.installationId`). */
  installationId?: number | null
  /** A credential the caller already resolved (and gated); absent = the
   *  forge resolves its own. */
  token?: string | null
}

/** A refused forge op. `status` = the forge's HTTP-like status (404 gone,
 *  409 head moved, 422 invalid, 405 not mergeable), `reason` = eCode's
 *  contract reason when the forge's answer names one (null = unknown). */
export class ForgeError extends Error {
  constructor(
    readonly status: number,
    message: string,
    readonly reason: ForgeReason | null = null
  ) {
    super(message)
    this.name = `ForgeError`
  }
}

/** A PR as the PR-tree ops list it. */
export interface ForgePullRef {
  number: number
  url: string
  headRef: string
  baseRef: string
}

/** A live PR as the repo's open-PR list shows it (the Reviews queue). */
export interface ForgeOpenPull extends ForgePullRef {
  title: string
  draft: boolean
  authorLogin: string | null
  authorAvatarUrl: string | null
  /** ISO time. */
  createdAt: string
}

/** One changed file, head against the merge base (`pr.files`). */
export interface ForgeFile {
  path: string
  /** Where a renamed/copied file came from. */
  previousPath: string | null
  change: `added` | `modified` | `removed` | `renamed` | `copied` | `changed` | `unchanged`
  additions: number
  deletions: number
  patch: string | null
}

/** How a device reaches the repo's git remote (`repositories.credentials`):
 *  the URL to fetch and push, and an HTTP basic credential for it. */
export interface ForgeCredentials {
  remoteUrl: string
  username: string
  token: string
  /** The token's real expiry (devices refresh before it). */
  expiresAt: Date
  /** GitHub: the installation the token was minted from. */
  installationId?: number | null
}

export interface ForgeMergeOrderedResult {
  merged: Array<{ number: number; sha: string | null }>
  /** The first refusal (the ones after it never ran); null = all landed. */
  failed: { number: number; reason: ForgeReason | null; message: string } | null
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
  /** False = conflicts with its base; null = not computed yet. */
  mergeable?: boolean | null
  /** The forge's last update of the PR. */
  updatedAt?: Date | null
  /** The PR's page. */
  url?: string | null
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

export type DirectMergeResult =
  | { ok: true; sha: string | null }
  | { ok: false; conflict: boolean; message: string; reason?: ForgeReason | null }

export interface Forge {
  readonly kind: Provider
  /** What this provider supports (`@exp/ecode` `supports()`): the UI and the
   *  playbook read these instead of assuming GitHub. */
  readonly capabilities: readonly Capability[]

  // ── the PR tree (forge.json) ──────────────────────────────────────────
  /** `pr.open`: a PR on ANY base. A second live PR for the head = ForgeError
   *  reason `exists`. */
  openPullRequest(
    repo: ForgeRepo,
    args: { head: string; base: string; title: string; body?: string; draft?: boolean }
  ): Promise<{ number: number; url: string }>
  /** The live PR on `head` (narrowed to `base` when given), null = none. */
  findOpenByHead(repo: ForgeRepo, head: string, base?: string): Promise<ForgePullRef | null>
  /** `pr.list_by_base`: the live PRs based on `base` (a stack parent's children). */
  listOpenByBase(repo: ForgeRepo, base: string): Promise<ForgePullRef[]>
  /** Every live PR of the repo (the base inference's candidates). */
  listOpen(repo: ForgeRepo): Promise<ForgeOpenPull[]>
  /** `pr.retarget`: a new base for a live PR. */
  retarget(repo: ForgeRepo, args: { number: number; base: string }): Promise<void>
  /** Flip a live PR between draft and ready. */
  setDraft(repo: ForgeRepo, args: { number: number; draft: boolean }): Promise<void>
  /** `pr.mergeability`: clean | dirty, null = not computed yet; `conflicts`
   *  = the paths when the forge names them. */
  mergeability(repo: ForgeRepo, number: number): Promise<{ mergeability: Mergeability | null; conflicts: string[] }>
  /** `pr.merge_ordered`: squash-merge in order, each its own transaction,
   *  stop at the first refusal. */
  mergeOrdered(
    repo: ForgeRepo,
    args: { numbers: number[]; expectedHeads?: Record<number, string> }
  ): Promise<ForgeMergeOrderedResult>
  /** `pr.close`: close without merging (children stay on its branch). */
  close(repo: ForgeRepo, args: { number: number }): Promise<void>
  /** `pr.files`: head against the merge base, in code-point path order.
   *  `fresh` skips any cache (a reader that must see a push from seconds ago). */
  files(repo: ForgeRepo, number: number, opts?: { fresh?: boolean }): Promise<ForgeFile[]>
  /** A branch's changes against `base` (no PR needed), null = the branch
   *  is not on the forge (not pushed, or gone). */
  branchFiles(repo: ForgeRepo, args: { base: string; branch: string }): Promise<ForgeFile[] | null>
  /** Rewrite a live PR's title and/or body (an omitted field keeps its value). */
  update(repo: ForgeRepo, args: { number: number; title?: string; body?: string }): Promise<void>
  /** Where `head` sits on top of the live PRs (`candidates`): the nearest
   *  one whose head is an ancestor of it, else `defaultBranch`. */
  inferBase(
    repo: ForgeRepo,
    args: {
      head: string
      defaultBranch: string
      candidates?: ReadonlyArray<{ number: number; branch: string }>
      stopBranches?: readonly string[]
    }
  ): Promise<{ base: string; prNumber: number | null }>
  /** A PR in a forge-native stack refuses a new base: take it out first
   *  (capability `native_stacks`; a no-op where there is none). */
  leaveNativeStack(repo: ForgeRepo, number: number): Promise<void>
  /** Is the PR in a forge-native stack? */
  inNativeStack(repo: ForgeRepo, number: number): Promise<boolean>

  /** The credential to act on (and clone, push) the repo; null = the forge
   *  cannot act there (not configured, no access). */
  credentials(repo: ForgeRepo): Promise<ForgeCredentials | null>

  // ── branches ──────────────────────────────────────────────────────────
  readPullRequest(repo: ForgeRepo, number: number): Promise<ForgePullRequest | null>
  /** The sha a branch points at, null when it does not exist. */
  readBranch(repo: ForgeRepo, branch: string): Promise<string | null>
  /** Delete a branch (a merged PR's head); `gone` = it no longer existed. */
  deleteBranch(repo: ForgeRepo, branch: string): Promise<`deleted` | `gone`>
  /** The repo's default branch as the forge reports it, null = unknown. */
  defaultBranch(repo: ForgeRepo): Promise<string | null>
  /** The base branch's REQUIRED check names (never hard-coded). */
  requiredChecks(repo: ForgeRepo, base: string): Promise<string[]>
  /** Start ONE pipeline run (merge-train.json `pipeline.inputs`). `missing` =
   *  the repo has no pipeline installed, or the forge lets this App run none
   *  there (#FEED-223): the train then lands directly. */
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

/** The forge of a repository: GitHub's, or eCode's own (#EXP-1616). */
export async function forgeFor(repo: ForgeRepo): Promise<Forge> {
  const forged = override?.(repo)
  if (forged) return forged
  if (repo.provider === `ecode`) {
    const { ecodeForge } = await import(`@/lib/ecode/forge-ecode`)
    return ecodeForge()
  }
  const { githubForge } = await import(`@/lib/ecode/forge-github`)
  return githubForge
}

/** Tests: route every forgeFor through `fake` (null restores the default). */
export function setForgeForTests(fake: ((repo: ForgeRepo) => Forge | null) | null): void {
  override = fake
}

/** Tests: a forge with only `ops`; any other op throws when called. */
export function partialForgeForTests(ops: Partial<Forge> & Pick<Forge, `kind`>): Forge {
  return new Proxy(ops as Forge, {
    get(target, key) {
      if (key in target) return target[key as keyof Forge]
      if (key === `capabilities`) return []
      if (typeof key !== `string` || key === `then`) return undefined
      return () => {
        throw new Error(`fake forge: ${key} is not implemented`)
      }
    },
  })
}
