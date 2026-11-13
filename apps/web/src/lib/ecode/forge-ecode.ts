// #EXP-1616 (eCode E3, #NEW-36): eCode's OWN forge. Every op of the Forge
// seam (`forge.ts`, the PR-tree primitives of packages/ecode
// `fixtures/forge.json`) implemented natively, with no code host behind it:
//
// - the team bucket IS the remote: refs are `repo_refs` rows moved by
//   compare-and-swap (the ONE serialization point), objects are packs in the
//   bucket that devices move over presigned URLs (`git-store.ts`);
// - PRs are our own `pull_requests` rows, numbered by `repositories.
//   pr_sequence`; the lifecycle (opened, retargeted, merged, closed) goes on
//   our own bus: the same pr-sync writers GitHub's webhooks call, so issue
//   status, wake-ups and the merge train behave exactly as with GitHub;
// - everything that needs git (mergeability, squash merge, files, `rebase
//   --onto`, ancestry, backup, mirror, repack) runs as a JOB on a team device
//   (`device_commands` kind `ecode_job`, `jobs.ts`): a job only WRITES
//   OBJECTS (a pack) and answers; every ref move stays a CAS here. No git
//   process ever runs on the server.
//
// The forge talks to the world through ONE `EcodeHost`: production's is
// `pgEcodeHost` (`ecode-host.ts`); forge-ecode-conformance.test.ts replays
// forge.json against an in-memory host whose device is eCode's reference
// model.
import type { TrainCheck } from "@exp/domain-contract/merge-train"
import { capabilityValues, supports, type ForgeReason, type Mergeability, type PrState } from "@exp/ecode"
import {
  ForgeError,
  type DirectMergeResult,
  type FastForwardResult,
  type Forge,
  type ForgeCredentials,
  type ForgeFile,
  type ForgeMergeOrderedResult,
  type ForgeOpenPull,
  type ForgePullRef,
  type ForgePullRequest,
  type ForgeRepo,
} from "@/lib/ecode/forge"

/** A PR as eCode's forge holds it (a `pull_requests` row of an ecode repo). */
export interface EcodePr {
  number: number
  url: string
  head: string
  base: string
  title: string
  body: string | null
  state: PrState
  /** The head's commit when it merged or closed. */
  headSha: string | null
  /** The squash commit on the base. */
  mergedSha: string | null
  createdAt: Date
  mergedAt: Date | null
  updatedAt: Date | null
  authorLogin: string | null
}

/** What the forge emits (contract `forgeEvent`), posted on our own bus. */
export type EcodeEvent =
  | { kind: `pr_opened`; number: number }
  | { kind: `pr_retargeted`; number: number; from: string; to: string }
  | { kind: `pr_merged`; number: number; sha: string }
  | { kind: `pr_closed`; number: number }
  | { kind: `branch_deleted`; branch: string }
  | { kind: `pushed`; branch: string; sha: string; forced?: boolean }

/** A git job for a team device; each answers a JSON result or refuses. */
export type EcodeJob =
  | { op: `mergeability`; base: string; head: string }
  /** `author` = `Name <email>` (#EXP-1617: the train's PR author); absent = the job's identity. */
  | { op: `squash`; base: string; head: string; message: string; author?: string }
  | { op: `files`; base: string; head: string }
  | { op: `rebase_onto`; tip: string; onto: string; upstream: string }
  | { op: `is_ancestor`; ancestor: string; of: string }
  /** #EXP-1617: a file at a commit (our CI's `.exponential/ci.json`). */
  | { op: `read_file`; sha: string; path: string }
  /** #EXP-1617: the files directly in a directory (our CI's workflows). */
  | { op: `list_dir`; sha: string; path: string }

export interface EcodeJobAnswers {
  mergeability: { mergeability: Mergeability; conflicts: string[] }
  squash: { sha: string }
  files: { files: ForgeFile[] }
  rebase_onto: { sha: string }
  is_ancestor: { yes: boolean }
  read_file: { text: string | null }
  list_dir: { files: string[] | null }
}

/** A device's answer: the result, or the spec's refusal. */
export type EcodeJobOutcome<K extends EcodeJob[`op`]> = { ok: true; result: EcodeJobAnswers[K] } | { ok: false; reason: ForgeReason; message: string }

/** One ref move: `expected` null = the branch must not exist; `sha` null = delete. */
export interface BranchMove {
  branch: string
  expected: string | null
  sha: string | null
}

/** Everything the forge needs from the world. */
export interface EcodeHost {
  /** branch → sha. */
  branches(repo: ForgeRepo): Promise<Record<string, string>>
  /** Atomic CAS over several branches: all move or none (false). */
  moveBranches(repo: ForgeRepo, moves: BranchMove[]): Promise<boolean>
  /** The next PR number (the per-repo sequence; never reused). */
  nextNumber(repo: ForgeRepo): Promise<number>
  pullUrl(repo: ForgeRepo, number: number): string
  prs(repo: ForgeRepo): Promise<EcodePr[]>
  /** Records a new PR (with the `pr_opened` event: the row exists after this). */
  recordOpened(repo: ForgeRepo, pr: { number: number; url: string; head: string; base: string; title: string; body: string | null; draft: boolean }): Promise<void>
  /** The forge's own fields of a PR (head/merged sha, title, body). */
  annotate(repo: ForgeRepo, number: number, patch: Partial<Pick<EcodePr, `headSha` | `mergedSha` | `title` | `body`>>): Promise<void>
  setDraft(repo: ForgeRepo, number: number, draft: boolean): Promise<void>
  /** Our own bus: applies the events (in order) the way webhooks would.
   *  `trainLanding` = the merge train landed it (its own step follows). */
  publish(repo: ForgeRepo, events: EcodeEvent[], opts?: { trainLanding?: boolean }): Promise<void>
  /** A git job on a team device. Throws ForgeError 503 when no device can run it. */
  job<K extends EcodeJob[`op`]>(repo: ForgeRepo, job: Extract<EcodeJob, { op: K }>): Promise<EcodeJobOutcome<K>>
  /** After a landing on `base`: the bundle backup and the mirror push (best effort). */
  landed(repo: ForgeRepo, base: string, sha: string): void
  defaultBranch(repo: ForgeRepo): Promise<string | null>
  requiredChecks(repo: ForgeRepo, base: string): Promise<string[]>
  checksAt(repo: ForgeRepo, sha: string): Promise<TrainCheck[] | null>
  credentials(repo: ForgeRepo): Promise<ForgeCredentials | null>
}

const LIVE: ReadonlySet<PrState> = new Set([`open`, `draft`])
const isLive = (pr: EcodePr) => LIVE.has(pr.state)

const STATUS: Record<ForgeReason, number> = {
  head: 422,
  base: 422,
  pr: 404,
  parent: 422,
  commit: 422,
  runner: 503,
  job: 503,
  same_branch: 422,
  exists: 422,
  cycle: 422,
  not_open: 422,
  not_running: 422,
  draft: 405,
  duplicate: 422,
  upstream: 422,
  head_moved: 409,
  dirty: 405,
  non_fast_forward: 409,
  commit_exists: 409,
  unsupported: 501,
}

function refuse(reason: ForgeReason, message: string = reason): never {
  throw new ForgeError(STATUS[reason], message, reason)
}

/** The ecode capabilities (contract `capability.byProvider`). */
export const ECODE_CAPABILITIES = capabilityValues.filter((c) => supports(`ecode`, c))

/** Would basing a PR with head `head` on `base` close a loop of live PRs? */
function closesLoop(prs: EcodePr[], head: string, base: string): boolean {
  let cursor = base
  for (let hops = 0; hops <= prs.length; hops++) {
    if (cursor === head) return true
    const parent = prs.find((pr) => isLive(pr) && pr.head === cursor)
    if (!parent) return false
    cursor = parent.base
  }
  return true
}

function pullRef(pr: EcodePr): ForgePullRef {
  return { number: pr.number, url: pr.url, headRef: pr.head, baseRef: pr.base }
}

/** A device refusal as the forge's error. */
function unwrap<K extends EcodeJob[`op`]>(outcome: EcodeJobOutcome<K>): EcodeJobAnswers[K] {
  if (!outcome.ok) refuse(outcome.reason, outcome.message)
  return outcome.result
}

/** eCode's forge over `host`. */
export function createEcodeForge(host: EcodeHost): Forge {
  async function livePr(repo: ForgeRepo, number: number): Promise<{ pr: EcodePr; prs: EcodePr[] }> {
    const prs = await host.prs(repo)
    const pr = prs.find((candidate) => candidate.number === number) ?? refuse(`pr`, `Pull request #${number} not found`)
    if (!isLive(pr)) refuse(`not_open`, `Pull request #${number} is ${pr.state}`)
    return { pr, prs }
  }

  async function tips(repo: ForgeRepo, pr: EcodePr): Promise<{ head: string; base: string; branches: Record<string, string> }> {
    const branches = await host.branches(repo)
    const head = branches[pr.head] ?? refuse(`head`, `The head branch ${pr.head} does not exist`)
    const base = branches[pr.base] ?? refuse(`base`, `The base branch ${pr.base} does not exist`)
    return { head, base, branches }
  }

  /** `pr.squash_merge`: ONE commit on the base (made by a device), the base
   *  moved by CAS, then the children onto the base and the head deleted. */
  async function squash(repo: ForgeRepo, number: number, expectedHead: string | null, title: string | null, trainLanding: boolean): Promise<string> {
    const { pr, prs } = await livePr(repo, number)
    if (pr.state === `draft`) refuse(`draft`, `Pull request #${number} is still a draft`)
    const branches = await host.branches(repo)
    const head = branches[pr.head] ?? refuse(`head`, `The head branch ${pr.head} does not exist`)
    if (expectedHead && expectedHead !== head) refuse(`head_moved`, `The head of #${number} moved`)
    const base = branches[pr.base] ?? refuse(`base`, `The base branch ${pr.base} does not exist`)
    const merge = unwrap(await host.job(repo, { op: `mergeability`, base, head }))
    if (merge.mergeability === `dirty`) refuse(`dirty`, `#${number} conflicts with ${pr.base}: ${merge.conflicts.join(`, `)}`)
    const message = `${title ?? pr.title} (#${number})`
    const { sha } = unwrap(await host.job(repo, { op: `squash`, base, head, message }))
    const defaultBranch = (await host.defaultBranch(repo)) ?? `main`
    const children = prs.filter((child) => isLive(child) && child.base === pr.head)
    // The head goes unless it is the default branch or another live PR's head.
    const deleteHead = pr.head !== defaultBranch && !prs.some((other) => other.number !== pr.number && isLive(other) && other.head === pr.head)
    const moves: BranchMove[] = [{ branch: pr.base, expected: base, sha }]
    if (deleteHead) moves.push({ branch: pr.head, expected: head, sha: null })
    if (!(await host.moveBranches(repo, moves))) {
      // Someone pushed the base or the head while the device worked.
      refuse(`head_moved`, `${pr.base} or ${pr.head} moved during the merge of #${number}`)
    }
    await host.annotate(repo, number, { headSha: head, mergedSha: sha })
    const events: EcodeEvent[] = [{ kind: `pr_merged`, number, sha }]
    for (const child of children) events.push({ kind: `pr_retargeted`, number: child.number, from: pr.head, to: pr.base })
    if (deleteHead) events.push({ kind: `branch_deleted`, branch: pr.head })
    await host.publish(repo, events, { trainLanding })
    host.landed(repo, pr.base, sha)
    return sha
  }

  async function filesBetween(repo: ForgeRepo, base: string, head: string): Promise<ForgeFile[]> {
    return unwrap(await host.job(repo, { op: `files`, base, head })).files
  }

  const forge: Forge = {
    kind: `ecode`,
    capabilities: ECODE_CAPABILITIES,

    async openPullRequest(repo, args) {
      const branches = await host.branches(repo)
      if (!branches[args.head]) refuse(`head`, `The head branch ${args.head} does not exist`)
      if (!branches[args.base]) refuse(`base`, `The base branch ${args.base} does not exist`)
      if (args.head === args.base) refuse(`same_branch`, `The head and the base are both ${args.head}`)
      const prs = await host.prs(repo)
      if (prs.some((pr) => isLive(pr) && pr.head === args.head)) refuse(`exists`, `A pull request already exists for ${args.head}`)
      if (closesLoop(prs, args.head, args.base)) refuse(`cycle`, `Basing ${args.head} on ${args.base} closes a loop`)
      const number = await host.nextNumber(repo)
      const url = host.pullUrl(repo, number)
      await host.recordOpened(repo, {
        number,
        url,
        head: args.head,
        base: args.base,
        title: args.title,
        body: args.body ?? null,
        draft: args.draft === true,
      })
      return { number, url }
    },

    async findOpenByHead(repo, head, base) {
      const pr = (await host.prs(repo)).find((p) => isLive(p) && p.head === head && (!base || p.base === base))
      return pr ? pullRef(pr) : null
    },

    async listOpenByBase(repo, base) {
      return (await host.prs(repo))
        .filter((pr) => isLive(pr) && pr.base === base)
        .sort((a, b) => a.number - b.number)
        .map(pullRef)
    },

    async listOpen(repo): Promise<ForgeOpenPull[]> {
      return (await host.prs(repo))
        .filter(isLive)
        .sort((a, b) => b.number - a.number)
        .map((pr) => ({
          ...pullRef(pr),
          title: pr.title,
          draft: pr.state === `draft`,
          authorLogin: pr.authorLogin,
          authorAvatarUrl: null,
          createdAt: pr.createdAt.toISOString(),
        }))
    },

    async retarget(repo, { number, base }) {
      const { pr, prs } = await livePr(repo, number)
      const branches = await host.branches(repo)
      if (!branches[base]) refuse(`base`, `The base branch ${base} does not exist`)
      if (base === pr.head) refuse(`same_branch`, `A pull request cannot be based on its own head`)
      if (base === pr.base) return
      if (closesLoop(prs, pr.head, base)) refuse(`cycle`, `Basing ${pr.head} on ${base} closes a loop`)
      await host.publish(repo, [{ kind: `pr_retargeted`, number, from: pr.base, to: base }])
    },

    async setDraft(repo, { number, draft }) {
      const { pr } = await livePr(repo, number)
      if ((pr.state === `draft`) !== draft) await host.setDraft(repo, number, draft)
    },

    async mergeability(repo, number) {
      const { pr } = await livePr(repo, number)
      const { head, base } = await tips(repo, pr)
      try {
        const merge = unwrap(await host.job(repo, { op: `mergeability`, base, head }))
        return { mergeability: merge.mergeability, conflicts: merge.conflicts }
      } catch (err) {
        // No device answered: not computed yet (the interface's null).
        if (err instanceof ForgeError && err.status === 503) return { mergeability: null, conflicts: [] }
        throw err
      }
    },

    async mergeOrdered(repo, { numbers, expectedHeads }): Promise<ForgeMergeOrderedResult> {
      if (new Set(numbers).size !== numbers.length) refuse(`duplicate`, `A pull request is listed twice`)
      const prs = await host.prs(repo)
      for (const number of numbers) {
        const pr = prs.find((p) => p.number === number) ?? refuse(`pr`, `Pull request #${number} not found`)
        if (!isLive(pr)) refuse(`not_open`, `Pull request #${number} is ${pr.state}`)
      }
      const merged: ForgeMergeOrderedResult[`merged`] = []
      for (const number of numbers) {
        try {
          merged.push({ number, sha: await squash(repo, number, expectedHeads?.[number] ?? null, null, false) })
        } catch (err) {
          if (!(err instanceof ForgeError) || !err.reason) throw err
          return { merged, failed: { number, reason: err.reason, message: err.message } }
        }
      }
      return { merged, failed: null }
    },

    async close(repo, { number }) {
      const { pr } = await livePr(repo, number)
      const branches = await host.branches(repo)
      await host.annotate(repo, number, { headSha: branches[pr.head] ?? null })
      await host.publish(repo, [{ kind: `pr_closed`, number }])
    },

    async files(repo, number) {
      const { pr } = await livePr(repo, number)
      const { head, base } = await tips(repo, pr)
      return filesBetween(repo, base, head)
    },

    async branchFiles(repo, { base, branch }) {
      const branches = await host.branches(repo)
      const head = branches[branch]
      const onto = branches[base]
      if (!head || !onto) return null
      return filesBetween(repo, onto, head)
    },

    async update(repo, { number, title, body }) {
      await livePr(repo, number)
      await host.annotate(repo, number, { ...(title !== undefined ? { title } : {}), ...(body !== undefined ? { body } : {}) })
    },

    async inferBase(repo, { head, defaultBranch, candidates, stopBranches }) {
      const branches = await host.branches(repo)
      const tip = branches[head]
      if (!tip) return { base: defaultBranch, prNumber: null }
      const live = candidates ?? (await host.prs(repo)).filter(isLive).map((pr) => ({ number: pr.number, branch: pr.head }))
      // Nearest first: the candidate with the most commits under `head`
      // wins; an ancestry probe per candidate (a device job each).
      let best: { base: string; prNumber: number; sha: string } | null = null
      for (const candidate of live) {
        if (candidate.branch === head || stopBranches?.includes(candidate.branch)) continue
        const sha = branches[candidate.branch]
        if (!sha) continue
        const under = unwrap(await host.job(repo, { op: `is_ancestor`, ancestor: sha, of: tip })).yes
        if (!under) continue
        if (best) {
          const nearer = unwrap(await host.job(repo, { op: `is_ancestor`, ancestor: best.sha, of: sha })).yes
          if (!nearer) continue
        }
        best = { base: candidate.branch, prNumber: candidate.number, sha }
      }
      return best ? { base: best.base, prNumber: best.prNumber } : { base: defaultBranch, prNumber: null }
    },

    // eCode has no native stacks: the PR tree IS its stacking.
    async leaveNativeStack() {},
    async inNativeStack() {
      return false
    },

    credentials: (repo) => host.credentials(repo),

    async readPullRequest(repo, number): Promise<ForgePullRequest | null> {
      const pr = (await host.prs(repo)).find((p) => p.number === number)
      if (!pr) return null
      const branches = await host.branches(repo)
      return {
        number,
        state: isLive(pr) ? `open` : `closed`,
        merged: pr.state === `merged`,
        draft: pr.state === `draft`,
        headSha: isLive(pr) ? (branches[pr.head] ?? null) : pr.headSha,
        headRef: pr.head,
        baseRef: pr.base,
        title: pr.title,
        mergedAt: pr.mergedAt,
        mergedBy: null,
        author: null,
        // Computed on demand (`mergeability`, a device job), never here.
        mergeable: null,
        updatedAt: pr.updatedAt,
        url: pr.url,
      }
    },

    async readBranch(repo, branch) {
      return (await host.branches(repo))[branch] ?? null
    },

    async deleteBranch(repo, branch) {
      const sha = (await host.branches(repo))[branch]
      if (!sha) return `gone`
      if (!(await host.moveBranches(repo, [{ branch, expected: sha, sha: null }]))) return `gone`
      await host.publish(repo, [{ kind: `branch_deleted`, branch }])
      return `deleted`
    },

    defaultBranch: (repo) => host.defaultBranch(repo),
    requiredChecks: (repo, base) => host.requiredChecks(repo, base),

    // The merge train's pipeline is eCode CI's (#EXP-1617, E4): until a repo
    // has one, the train lands directly (`squashMerge`, a device job).
    // #EXP-1617: a file at a commit is a device job (the packs live in the
    // bucket, a team device reads them with LocalRepo). A branch name is
    // resolved here first: the device reads commits only.
    async readFile(repo, path, ref) {
      const sha = /^[0-9a-f]{40}$/.test(ref) ? ref : ((await host.branches(repo))[ref] ?? null)
      if (!sha) return null
      return unwrap(await host.job(repo, { op: `read_file`, sha, path })).text
    },
    async listFiles(repo, dir, ref) {
      const sha = /^[0-9a-f]{40}$/.test(ref) ? ref : ((await host.branches(repo))[ref] ?? null)
      if (!sha) return null
      return unwrap(await host.job(repo, { op: `list_dir`, sha, path: dir })).files
    },
    // Our CI's check rows ARE this forge's checks: nothing to mirror.
    async reportStatus() {},

    // #EXP-1617: our own CI runs the train (lib/ci/train.ts): the batch
    // composed by device jobs, then a `merge_train` run on its head.
    async dispatchPipeline(repo, inputs) {
      const { dispatchEcodeTrain } = await import(`@/lib/ci/train`)
      return dispatchEcodeTrain(host, repo, inputs)
    },
    async readPipeline(_repo, _trainId, runRef) {
      const { readEcodeTrain } = await import(`@/lib/ci/train`)
      return readEcodeTrain(runRef)
    },

    checksAt: (repo, sha) => host.checksAt(repo, sha),

    async branchContains(repo, branch, sha) {
      const tip = (await host.branches(repo))[branch]
      if (!tip) return false
      if (tip === sha) return true
      try {
        return unwrap(await host.job(repo, { op: `is_ancestor`, ancestor: sha, of: tip })).yes
      } catch {
        return null
      }
    },

    async fastForward(repo, { branch, from, to }): Promise<FastForwardResult> {
      // The train composed `to` on top of `from`; the CAS on `from` is the
      // fast-forward guard (a landing in between = moved).
      if (await host.moveBranches(repo, [{ branch, expected: from, sha: to }])) {
        await host.publish(repo, [{ kind: `pushed`, branch, sha: to }])
        host.landed(repo, branch, to)
        return { ok: true }
      }
      return { ok: false, reason: `moved`, current: (await host.branches(repo))[branch] ?? null }
    },

    async closeAsLanded(repo, { number, sha }) {
      // The train merged the row already: the forge records the commit and
      // drops the head (children were moved by the train's own step).
      const pr = (await host.prs(repo)).find((p) => p.number === number)
      if (!pr) return
      const branches = await host.branches(repo)
      await host.annotate(repo, number, { headSha: branches[pr.head] ?? pr.headSha, mergedSha: sha })
      const defaultBranch = await host.defaultBranch(repo)
      const head = branches[pr.head]
      const stillUsed = (await host.prs(repo)).some((p) => isLive(p) && (p.head === pr.head || p.base === pr.head))
      if (head && pr.head !== defaultBranch && !stillUsed && (await host.moveBranches(repo, [{ branch: pr.head, expected: head, sha: null }]))) {
        await host.publish(repo, [{ kind: `branch_deleted`, branch: pr.head }])
      }
    },

    async squashMerge(repo, { number, title, expectedHead }): Promise<DirectMergeResult> {
      try {
        return { ok: true, sha: await squash(repo, number, expectedHead, title, true) }
      } catch (err) {
        if (!(err instanceof ForgeError)) throw err
        return { ok: false, conflict: err.reason === `dirty`, message: err.message, reason: err.reason }
      }
    },
  }
  return forge
}

/** Production's ecode forge (lazy: the host pulls in the database). */
let production: Forge | null = null
export async function ecodeForge(): Promise<Forge> {
  if (production) return production
  const { pgEcodeHost } = await import(`@/lib/ecode/ecode-host`)
  production = createEcodeForge(pgEcodeHost)
  return production
}

/** `rebase --onto` for a dirty child after its parent landed (capability
 *  `rebase_onto`): the device replays `branch` after `upstream` onto `onto`,
 *  the branch moves by CAS (a forced push). */
export async function rebaseOnto(
  host: EcodeHost,
  repo: ForgeRepo,
  args: { branch: string; onto: string; upstream: string }
): Promise<string> {
  const branches = await host.branches(repo)
  const tip = branches[args.branch] ?? refuse(`head`, `The branch ${args.branch} does not exist`)
  const onto = branches[args.onto] ?? refuse(`base`, `The branch ${args.onto} does not exist`)
  const { sha } = unwrap(await host.job(repo, { op: `rebase_onto`, tip, onto, upstream: args.upstream }))
  if (!(await host.moveBranches(repo, [{ branch: args.branch, expected: tip, sha }]))) {
    refuse(`head_moved`, `${args.branch} moved during the rebase`)
  }
  await host.publish(repo, [{ kind: `pushed`, branch: args.branch, sha, forced: true }])
  return sha
}
