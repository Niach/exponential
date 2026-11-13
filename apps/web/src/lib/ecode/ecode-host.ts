// #EXP-1616: the production EcodeHost behind `ecodeForge`. Refs are
// `repo_refs` rows (CAS, `git-store.ts`); PRs are `pull_requests` rows of
// the ecode repo; the bus is pr-sync's writers, the same ones GitHub's
// webhooks call (so issue status, wake-ups, the `pr.*` events and the merge
// train behave exactly as with GitHub); git jobs go to a team device
// (`jobs.ts`); a landing queues the bundle backup and the mirror push.
import { and, eq, sql } from "drizzle-orm"
import type { TrainCheck } from "@exp/domain-contract/merge-train"
import { db } from "@/db/connection"
import { commitChecks, pullRequests, repositories } from "@/db/schema"
import type { ForgeCredentials, ForgeRepo } from "@/lib/ecode/forge"
import type { EcodeEvent, EcodeHost, EcodePr } from "@/lib/ecode/forge-ecode"
import { HEADS, compareAndSwapRefs, listRefs } from "@/lib/ecode/git-store"
import { queueMaintenance, runJob } from "@/lib/ecode/jobs"
import { appBaseUrl } from "@/lib/notification-email-policy"

interface RepoFacts {
  id: string
  teamId: string
  fullName: string
  defaultBranch: string
  providerConfig: Record<string, unknown>
}

async function facts(repo: ForgeRepo): Promise<RepoFacts> {
  const [row] = await db
    .select({
      id: repositories.id,
      teamId: repositories.teamId,
      fullName: repositories.fullName,
      defaultBranch: sql<string>`coalesce(${repositories.defaultBranchOverride}, ${repositories.defaultBranch})`,
      providerConfig: repositories.providerConfig,
    })
    .from(repositories)
    .where(eq(repositories.id, repo.id))
    .limit(1)
  if (!row) throw new Error(`repository ${repo.id} not found`)
  return { ...row, providerConfig: (row.providerConfig ?? {}) as Record<string, unknown> }
}

/** `https://<app>/t/<team>/repos/<name>/pulls/<n>`: unique (the PR url key)
 *  and the repo page's own route (`repoFromPrUrl` reads it back). */
export function ecodePullUrl(fullName: string, number: number): string {
  const [team, name] = fullName.split(`/`)
  return `${appBaseUrl()}/t/${team}/repos/${name}/pulls/${number}`
}

async function rowId(repo: ForgeRepo, number: number): Promise<string | null> {
  const [row] = await db
    .select({ id: pullRequests.id })
    .from(pullRequests)
    .where(and(eq(pullRequests.repositoryId, repo.id), eq(pullRequests.number, number)))
    .limit(1)
  return row?.id ?? null
}

/** The mirror settings in `provider_config` (`mirrorUrl`, off when unset). */
export function mirrorOf(config: Record<string, unknown>): { url: string } | null {
  const url = typeof config.mirrorUrl === `string` ? config.mirrorUrl.trim() : ``
  return url && config.mirrorEnabled !== false ? { url } : null
}

/** The mirror push (every branch and tag, forced, pruned), when the repo
 *  has a mirror; best effort, one pending per repo. */
function queueMirror(repo: ForgeRepo): void {
  void (async () => {
    const repoFacts = await facts(repo)
    const mirror = mirrorOf(repoFacts.providerConfig)
    if (!mirror) return
    const { mirrorJob } = await import(`@/lib/ecode/mirror`)
    await queueMaintenance(repoFacts, await mirrorJob(mirror.url))
  })().catch((err: unknown) => console.error(`[ecode] queuing the mirror of ${repo.fullName} failed`, err))
}

export const pgEcodeHost: EcodeHost = {
  async branches(repo) {
    const refs = await listRefs(repo.id)
    const out: Record<string, string> = {}
    for (const [ref, sha] of Object.entries(refs)) if (ref.startsWith(HEADS)) out[ref.slice(HEADS.length)] = sha
    return out
  },

  moveBranches(repo, moves) {
    return compareAndSwapRefs(
      repo.id,
      moves.map((m) => ({ ref: `${HEADS}${m.branch}`, old: m.expected, new: m.sha }))
    )
  },

  async nextNumber(repo) {
    // Never below a number a row already holds (rows imported from a mirror).
    const [row] = await db
      .update(repositories)
      .set({
        prSequence: sql`greatest(${repositories.prSequence}, (select coalesce(max(${pullRequests.number}), 0) from ${pullRequests} where ${pullRequests.repositoryId} = ${repo.id})) + 1`,
      })
      .where(eq(repositories.id, repo.id))
      .returning({ n: repositories.prSequence })
    return row!.n
  },

  pullUrl: (repo, number) => ecodePullUrl(repo.fullName, number),

  async prs(repo): Promise<EcodePr[]> {
    const rows = await db
      .select({
        number: pullRequests.number,
        url: pullRequests.url,
        head: pullRequests.head,
        base: pullRequests.base,
        title: pullRequests.title,
        body: pullRequests.body,
        state: pullRequests.state,
        headSha: pullRequests.headSha,
        mergedSha: pullRequests.mergedSha,
        createdAt: pullRequests.createdAt,
        mergedAt: pullRequests.mergedAt,
        updatedAt: pullRequests.updatedAt,
      })
      .from(pullRequests)
      .where(eq(pullRequests.repositoryId, repo.id))
    return rows.map((row) => ({ ...row, title: row.title ?? ``, authorLogin: null }))
  },

  async recordOpened(repo, pr) {
    const { teamId } = await facts(repo)
    const { findPrLinkIssueIds, recordPullRequestOpened } = await import(`@/lib/integrations/pr-sync`)
    // The `pr_opened` event: what GitHub's `opened` webhook records (the
    // issues its branch, title and body name). The in-app opener's own
    // record then gives the row its chat (pr-sync's "webhook first" path).
    const links = await findPrLinkIssueIds(repo.fullName, { head: pr.head, title: pr.title, body: pr.body })
    await recordPullRequestOpened({
      teamId,
      rootSessionId: null,
      sessionId: null,
      repositoryId: repo.id,
      repoFullName: repo.fullName,
      number: pr.number,
      url: pr.url,
      title: pr.title,
      head: pr.head,
      base: pr.base,
      draft: pr.draft,
      issueIds: links?.teamId === teamId ? links.issueIds : [],
      actorUserId: null,
    })
    if (pr.body) {
      await db
        .update(pullRequests)
        .set({ body: pr.body })
        .where(and(eq(pullRequests.repositoryId, repo.id), eq(pullRequests.number, pr.number)))
    }
  },

  async annotate(repo, number, patch) {
    if (Object.keys(patch).length === 0) return
    await db
      .update(pullRequests)
      .set({ ...patch, updatedAt: new Date() })
      .where(and(eq(pullRequests.repositoryId, repo.id), eq(pullRequests.number, number)))
  },

  async setDraft(repo, number, draft) {
    const id = await rowId(repo, number)
    if (!id) return
    const { applyPullRequestDraft } = await import(`@/lib/integrations/pr-sync`)
    await applyPullRequestDraft({ pullRequestId: id, draft })
  },

  async publish(repo, events: EcodeEvent[], opts) {
    const sync = await import(`@/lib/integrations/pr-sync`)
    for (const event of events) {
      switch (event.kind) {
        case `pr_merged`: {
          const id = await rowId(repo, event.number)
          if (id) await sync.applyPullRequestMerged({ pullRequestId: id, mergedAt: new Date(), actorUserId: null, trainLanding: opts?.trainLanding === true })
          break
        }
        case `pr_closed`: {
          const id = await rowId(repo, event.number)
          if (id) await sync.applyPullRequestClosed({ pullRequestId: id })
          break
        }
        case `pr_retargeted`: {
          const id = await rowId(repo, event.number)
          if (id) await sync.applyPullRequestBase({ pullRequestId: id, base: event.to })
          break
        }
        case `pushed`: {
          // GitHub's `push` webhook: the session on that branch gets the credit.
          const { attributePush } = await import(`@/lib/sessions/push-attribution`)
          await attributePush({ fullName: repo.fullName, branch: event.branch }).catch(() => null)
          break
        }
        // `pr_opened` was recorded by `recordOpened`.
        default:
          break
      }
    }
    // A branch deleted here (a push, a merged head, deleteBranch) is
    // deleted on the mirror too: its push prunes (#EXP-1616).
    if (events.some((event) => event.kind === `branch_deleted`)) queueMirror(repo)
  },

  async job(repo, job) {
    const { teamId } = await facts(repo)
    return runJob({ ...repo, teamId }, job)
  },

  landed(repo) {
    // A bundle snapshot per landing, then the mirror push (best effort;
    // a device runs both when one is online).
    void (async () => {
      await queueMaintenance(await facts(repo), { op: `backup` })
    })().catch((err: unknown) => console.error(`[ecode] queuing the backup of ${repo.fullName} failed`, err))
    queueMirror(repo)
  },

  async defaultBranch(repo) {
    return (await facts(repo)).defaultBranch
  },

  async requiredChecks(repo) {
    const { providerConfig } = await facts(repo)
    const names = providerConfig.requiredChecks
    return Array.isArray(names) ? names.filter((n): n is string => typeof n === `string`) : []
  },

  async checksAt(repo, sha): Promise<TrainCheck[]> {
    // eCode CI (#EXP-1617) writes commit_checks itself: the rows ARE the truth.
    const rows = await db
      .select({ name: commitChecks.name, status: commitChecks.status, conclusion: commitChecks.conclusion })
      .from(commitChecks)
      .where(and(eq(commitChecks.repositoryId, repo.id), eq(commitChecks.sha, sha)))
    return rows.map((r) => ({ name: r.name, status: r.status, conclusion: r.conclusion }))
  },

  // A repo credential is minted per MEMBER (`repositories.credentials`);
  // the server itself never acts as one.
  async credentials(): Promise<ForgeCredentials | null> {
    return null
  },
}
