import { createHmac, timingSafeEqual } from "node:crypto"
import { createFileRoute } from "@tanstack/react-router"
import { and, eq, inArray, isNull, or } from "drizzle-orm"
import { db } from "@/db/connection"
import {
  githubInstallationLinks,
  githubInstallationRepoGrants,
  githubInstallations,
  issues,
  repositories,
} from "@/db/schema"
import { resolveRepoDefaultBranchCached } from "@/lib/integrations/github-app"
import { getTeamMember } from "@/lib/team-membership"
import {
  applyPrClosedState,
  applyPrMergeState,
  applyPrOpenedState,
  applyPrReopenedState,
  applySessionPrState,
  findIssueIdByBranch,
} from "@/lib/integrations/pr-sync"
import {
  takePrMergeClaim,
  takePrOpenClaim,
} from "@/lib/integrations/pr-actor-claims"
import {
  resolveAppUserForGithubActor,
  type GithubActorRef,
} from "@/lib/integrations/github-identity"
import { invalidateRepoCacheForInstallation } from "@/lib/trpc/integrations"

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": `application/json` },
  })
}

// Verify GitHub's `x-hub-signature-256` HMAC-SHA256 over the raw request body.
// Constant-time comparison guards against timing attacks. Returns false on any
// shape/length mismatch (timingSafeEqual throws on unequal-length buffers).
function verifySignature(rawBody: string, signature: string, secret: string) {
  const expected = `sha256=${createHmac(`sha256`, secret)
    .update(rawBody)
    .digest(`hex`)}`
  const a = Buffer.from(signature)
  const b = Buffer.from(expected)
  if (a.length !== b.length) return false
  return timingSafeEqual(a, b)
}

// Resolve the payload's PR to our issues: exact prUrl match first — plural,
// because a batch coding run links ALL its issues to ONE combined PR — then a
// deterministic single-issue fallback that parses the `exp/<IDENTIFIER>` head
// branch and resolves through the repositories registry, so PRs opened
// out-of-band still link. (Batch branches are `exp/batch-<hex>` — lowercase by
// construction, so the branch parse can never mis-link them.)
async function resolveIssuesForPr(args: {
  htmlUrl: string
  repoFullName?: string
  headRef?: string
}): Promise<string[]> {
  const byUrl = await db
    .select({ id: issues.id })
    .from(issues)
    .where(eq(issues.prUrl, args.htmlUrl))
  if (byUrl.length > 0) return byUrl.map((row) => row.id)
  if (args.repoFullName && args.headRef) {
    const issueId = await findIssueIdByBranch(args.repoFullName, args.headRef)
    return issueId ? [issueId] : []
  }
  return []
}

// FEED-30: keep the grant snapshot in step with GitHub's repo selection. The
// `installation_repositories` sender just changed which repos this
// installation grants, so they control it and can access what they picked —
// write THEIR grant rows for every linked team they belong to, and the
// picker's Refresh shows a freshly granted repo without an OAuth re-auth (the
// only other grant writer). Best-effort: an unmapped sender (never connected
// a GitHub account here) or a missed delivery just leaves the re-auth path.
async function syncGrantsForAddedRepos(
  installationId: number,
  added: Array<{ fullName: string; private: boolean }>,
  sender: GithubActorRef | undefined
): Promise<void> {
  if (added.length === 0) return
  const userId = await resolveAppUserForGithubActor(sender)
  if (!userId) return
  const linked = await db
    .select({ teamId: githubInstallationLinks.teamId })
    .from(githubInstallationLinks)
    .innerJoin(
      githubInstallations,
      eq(githubInstallations.id, githubInstallationLinks.githubInstallationId)
    )
    .where(eq(githubInstallations.installationId, installationId))
  const teamIds = [...new Set(linked.map((row) => row.teamId))]
  // Membership FIRST: only the linked teams the sender belongs to get rows,
  // and a sender who belongs to none must not cost a GitHub call per repo.
  const memberTeamIds: string[] = []
  for (const teamId of teamIds) {
    if (await getTeamMember(userId, teamId)) memberTeamIds.push(teamId)
  }
  if (memberTeamIds.length === 0) return
  const defaultBranches = new Map<string, string | null>()
  for (const repo of added) {
    try {
      defaultBranches.set(
        repo.fullName,
        await resolveRepoDefaultBranchCached(repo.fullName)
      )
    } catch {
      defaultBranches.set(repo.fullName, null)
    }
  }
  const rows: Array<typeof githubInstallationRepoGrants.$inferInsert> = []
  for (const teamId of memberTeamIds) {
    for (const repo of added) {
      rows.push({
        teamId,
        installationId,
        fullName: repo.fullName,
        private: repo.private,
        defaultBranch: defaultBranches.get(repo.fullName) ?? null,
        grantedByUserId: userId,
      })
    }
  }
  if (rows.length === 0) return
  await db.insert(githubInstallationRepoGrants).values(rows).onConflictDoNothing()
}

// GitHub webhook receiver — the CLOUD PR-linking + merge-detection trigger
// (self-hosted uses the outbound cron for merges instead). Acts on
// `installation` `created`/`unsuspend` (upsert + clear the suspension mark),
// `suspend` (mark, never delete — REV2-29) and `deleted` (drop the row and its
// claim links), `installation_repositories` (repo-selection
// changes → flag/heal `repositories.inaccessible_at`), `pull_request` `opened`
// (link an out-of-band PR to its issue) and `closed` (flip prState to merged
// or, when closed without merging, to closed).
// Issues resolve by exact prUrl OR by the `exp/<IDENTIFIER>` head-branch
// parse; everything else is acked and ignored.
async function handleGithubWebhook(request: Request): Promise<Response> {
  try {
    const secret = process.env.GITHUB_WEBHOOK_SECRET
    if (!secret) {
      return jsonResponse(503, { error: `webhook not configured` })
    }

    const signature = request.headers.get(`x-hub-signature-256`)
    const rawBody = await request.text()
    if (!signature || !verifySignature(rawBody, signature, secret)) {
      return jsonResponse(401, { error: `invalid signature` })
    }

    const event = request.headers.get(`x-github-event`)

    // App lifecycle: mirror installs into github_installations. The setup
    // redirect is best-effort (it can land without a browser session, or not
    // at all) — this webhook is the reliable writer for the UI "installed"
    // state. User attribution stays null here (webhooks carry no app user);
    // the setup redirect fills it in when it can.
    if (event === `installation`) {
      const payload = JSON.parse(rawBody) as {
        action?: string
        installation?: { id?: number; account?: { login?: string; type?: string } }
      }
      const installation = payload.installation
      if (!installation?.id) {
        return jsonResponse(200, { ok: true })
      }
      if (payload.action === `created` || payload.action === `unsuspend`) {
        // `unsuspend` HEALS: the row and every team's claim link survived the
        // suspension (see the `suspend` branch), so clearing `suspended_at`
        // restores discovery/connect/token minting with no manual reconnect.
        // Repo `inaccessible_at` flags stay put — only a VERIFIED access proof
        // clears those (the token mint, repositories.list's default-branch
        // heal), and both are reachable again now that the links are intact.
        await db
          .insert(githubInstallations)
          .values({
            installationId: installation.id,
            accountLogin: installation.account?.login ?? null,
            accountType: installation.account?.type ?? null,
            suspendedAt: null,
          })
          .onConflictDoUpdate({
            target: githubInstallations.installationId,
            set: {
              accountLogin: installation.account?.login ?? null,
              accountType: installation.account?.type ?? null,
              suspendedAt: null,
              updatedAt: new Date(),
            },
          })
      } else if (payload.action === `suspend`) {
        // REVERSIBLE (REV2-29): GitHub keeps the installation and only refuses
        // to mint tokens for it, so this must NOT delete the row — that
        // CASCADE-dropped every team's claim link, and the `unsuspend`
        // re-insert minted a fresh uuid PK the old links could never point at
        // again. Mark it suspended instead (discovery/connect go inert) and
        // flag every bound repo as inaccessible so the settings UI stops
        // showing them healthy while every token mint fails.
        await db
          .update(githubInstallations)
          .set({ suspendedAt: new Date(), updatedAt: new Date() })
          .where(eq(githubInstallations.installationId, installation.id))
        await db
          .update(repositories)
          .set({ inaccessibleAt: new Date() })
          .where(eq(repositories.installationId, installation.id))
      } else if (payload.action === `deleted`) {
        // TERMINAL: the App was uninstalled. Flag every repo bound to it as
        // inaccessible (the settings badge + the launcher's 412), then drop the
        // row — its team links CASCADE away with it, and a re-install gets a
        // brand-new installation id anyway, so nothing could have been restored.
        // Invalidate the repo cache BEFORE the delete: the invalidation
        // resolves the linked teams through the installation links, which
        // cascade away with the row.
        await invalidateRepoCacheForInstallation(installation.id)
        await db
          .update(repositories)
          .set({ inaccessibleAt: new Date() })
          .where(eq(repositories.installationId, installation.id))
        await db
          .delete(githubInstallations)
          .where(eq(githubInstallations.installationId, installation.id))
      }
      await invalidateRepoCacheForInstallation(installation.id)
      return jsonResponse(200, { ok: true })
    }

    // Repo-selection changes on an installation ("Only select repositories").
    // `removed` repos lose token access instantly — flag their registry rows so
    // the settings UI shows the no-access badge instead of the launcher
    // discovering it at clone time. `added` repos regain access — clear the
    // flag and heal a stale/NULL installation binding (match by full_name so
    // rows connected before this webhook existed heal too).
    if (event === `installation_repositories`) {
      const payload = JSON.parse(rawBody) as {
        action?: string
        installation?: { id?: number; account?: { login?: string; type?: string } }
        repositories_added?: Array<{ full_name?: string; private?: boolean }>
        repositories_removed?: Array<{ full_name?: string }>
        sender?: GithubActorRef
      }
      const installation = payload.installation
      if (!installation?.id) {
        return jsonResponse(200, { ok: true })
      }
      // Keep the mirror row fresh (this event can arrive before any
      // install/setup round-trip on instances that added the webhook late).
      await db
        .insert(githubInstallations)
        .values({
          installationId: installation.id,
          accountLogin: installation.account?.login ?? null,
          accountType: installation.account?.type ?? null,
        })
        .onConflictDoNothing()

      const added = (payload.repositories_added ?? [])
        .map((r) => r.full_name)
        .filter((name): name is string => Boolean(name))
      const removed = (payload.repositories_removed ?? [])
        .map((r) => r.full_name)
        .filter((name): name is string => Boolean(name))

      if (removed.length > 0) {
        await db
          .update(repositories)
          .set({ inaccessibleAt: new Date() })
          .where(
            and(
              eq(repositories.installationId, installation.id),
              inArray(repositories.fullName, removed)
            )
          )
        // FEED-30: nobody reaches a removed repo through the App anymore —
        // drop every member's grant rows for it so the pickers stop listing
        // it (the OAuth re-auth would only scrub the re-authing user's own).
        await db
          .delete(githubInstallationRepoGrants)
          .where(
            and(
              eq(githubInstallationRepoGrants.installationId, installation.id),
              inArray(githubInstallationRepoGrants.fullName, removed)
            )
          )
      }
      if (added.length > 0) {
        // Tenant-scoped heal (EXP-363 hardening): a bare full_name match let
        // ANY installation of the App rebind another team's registry row (a
        // renamed-then-squatted account could clear the no-access flag and
        // repoint installation_id from the outside). Heal only rows that are
        // already bound to THIS installation, still unbound (connected before
        // this webhook existed), or owned by a team that has actually claimed
        // this installation — the same trust root every token mint checks.
        const claimingTeams = db
          .select({ teamId: githubInstallationLinks.teamId })
          .from(githubInstallationLinks)
          .innerJoin(
            githubInstallations,
            eq(
              githubInstallations.id,
              githubInstallationLinks.githubInstallationId
            )
          )
          .where(eq(githubInstallations.installationId, installation.id))
        await db
          .update(repositories)
          .set({ inaccessibleAt: null, installationId: installation.id })
          .where(
            and(
              inArray(repositories.fullName, added),
              or(
                isNull(repositories.installationId),
                eq(repositories.installationId, installation.id),
                inArray(repositories.teamId, claimingTeams)
              )
            )
          )
        // Best-effort for real: a failed grant sync must neither turn the
        // delivery into a 500 (GitHub would retry the whole heal) nor skip
        // the cache invalidation below. The OAuth re-auth path remains.
        try {
          await syncGrantsForAddedRepos(
            installation.id,
            (payload.repositories_added ?? [])
              .filter((r): r is { full_name: string; private?: boolean } =>
                Boolean(r.full_name)
              )
              .map((r) => ({
                fullName: r.full_name,
                private: r.private === true,
              })),
            payload.sender
          )
        } catch (err) {
          console.error(
            `[github-webhook] grant sync for installation ${installation.id} failed:`,
            err
          )
        }
      }
      await invalidateRepoCacheForInstallation(installation.id)
      return jsonResponse(200, { ok: true })
    }

    if (event !== `pull_request`) {
      return jsonResponse(200, { ok: true })
    }

    const payload = JSON.parse(rawBody) as {
      action?: string
      pull_request?: {
        html_url?: string
        number?: number
        merged?: boolean
        merged_at?: string | null
        head?: { ref?: string }
        // EXP-617: the GitHub identity behind the event. `user` is the PR
        // author, `merged_by` whoever pressed Merge; both are the App bot for
        // anything our own server did, which is exactly why they complement
        // the actor claims instead of competing with them.
        user?: GithubActorRef
        merged_by?: GithubActorRef | null
      }
      repository?: { full_name?: string }
      sender?: GithubActorRef
    }

    const pr = payload.pull_request
    if (!pr?.html_url) {
      return jsonResponse(200, { ok: true })
    }
    const htmlUrl = pr.html_url
    const repoFullName = payload.repository?.full_name
    const headRef = pr.head?.ref

    // Merge: flip prState → merged, stamp prMergedAt, emit pr_merged (once
    // per issue). A batch PR resolves to every linked issue — merging it
    // completes them all (each apply is idempotent per issue).
    if (payload.action === `closed` && pr.merged === true) {
      const mergedAt = pr.merged_at ? new Date(pr.merged_at) : new Date()
      const issueIds = await resolveIssuesForPr({
        htmlUrl,
        repoFullName,
        headRef,
      })
      // EXP-494: a merge initiated in-app (issues.mergePr /
      // repositories.mergePull) claimed its actor right before the GitHub
      // call — resolve it ONCE per delivery (one claim covers every issue of
      // a batch PR) so this leg fires attributed and the initiator is
      // excluded even when no coding_sessions row survived to fall back on.
      const claim =
        repoFullName && pr.number
          ? takePrMergeClaim(repoFullName, pr.number)
          : null
      // EXP-617: whoever actually pressed Merge on github.com, if they have
      // ever connected that GitHub account here. `merged_by` names the button
      // presser; `sender` is the fallback for a payload that omits it.
      // Resolved ONCE per delivery, like the claim. Never `pull_request.user`
      // — a PR author absolutely wants to hear that their PR landed.
      const githubActorUserId = await resolveAppUserForGithubActor(
        pr.merged_by ?? payload.sender
      )
      // EXP-711: the claim also carries the merger's per-call `endSessions`
      // override, so the echo of an in-app merge honours it.
      const endSessions = claim?.endSessions
      // EXP-637/EXP-626/EXP-734: an issue-LESS chore PR (opened with
      // `exponential_pr_open({ repositoryId, head })`) resolves to no issue
      // above — it lives on the coding_sessions row that opened it, so flip
      // it to merged and end the run there (idempotent against the in-app
      // merge helper that already did so), except the session that merged
      // its own PR, which lives on until sessions_end.
      await applySessionPrState({
        prUrl: htmlUrl,
        state: `merged`,
        ...(endSessions !== undefined ? { endSessions } : {}),
      })
      for (const issueId of issueIds) {
        await applyPrMergeState({
          githubActorUserId,
          issueId,
          prUrl: htmlUrl,
          ...(endSessions !== undefined ? { endSessions } : {}),
          // Backfill sources for a never-linked issue (branch-parse fallback
          // whose 'opened' webhook was lost) — see applyPrMergeState (REV-26).
          ...(pr.number != null ? { prNumber: pr.number } : {}),
          ...(headRef ? { headBranch: headRef } : {}),
          mergedAt,
          ...(claim
            ? { actorUserId: claim.userId, actorViaAgent: claim.viaAgent }
            : { actorUserId: null }),
        })
      }
      return jsonResponse(200, { ok: true })
    }

    // Closed without merging: flip prState → closed so the issue leaves the
    // Reviews open-PR surfaces (state-only; no pr_closed event type exists).
    if (payload.action === `closed` && pr.merged !== true) {
      const issueIds = await resolveIssuesForPr({
        htmlUrl,
        repoFullName,
        headRef,
      })
      for (const issueId of issueIds) {
        await applyPrClosedState({ issueId, prUrl: htmlUrl })
      }
      await applySessionPrState({ prUrl: htmlUrl, state: `closed` })
      return jsonResponse(200, { ok: true })
    }

    // Reopened after a close-without-merge: heal closed → open.
    if (payload.action === `reopened`) {
      const issueIds = await resolveIssuesForPr({
        htmlUrl,
        repoFullName,
        headRef,
      })
      for (const issueId of issueIds) {
        await applyPrReopenedState({ issueId, prUrl: htmlUrl })
      }
      await applySessionPrState({ prUrl: htmlUrl, state: `open` })
      return jsonResponse(200, { ok: true })
    }

    // Opened out-of-band: link the PR to its issue if it has none yet.
    if (payload.action === `opened` && repoFullName && headRef && pr.number) {
      const issueIds = await resolveIssuesForPr({
        htmlUrl,
        repoFullName,
        headRef,
      })
      // EXP-494: the MCP open_pr tool claimed its actor (keyed on the head
      // branch — the PR number doesn't exist before creation) so this leg
      // fires attributed with a title byte-identical to the tool's own
      // fan-out; deliver()'s dedupe window collapses the racing pair.
      const claim = takePrOpenClaim(repoFullName, headRef)
      // EXP-617: for a PR opened OUT OF BAND (an agent running `gh` under the
      // developer's own credentials, or the compare view on github.com) this
      // is the only signal that names a human — our own open_pr calls carry
      // the App bot and resolve to nobody, leaving the claim in charge.
      const githubActorUserId = await resolveAppUserForGithubActor(
        pr.user ?? payload.sender
      )
      for (const issueId of issueIds) {
        await applyPrOpenedState({
          githubActorUserId,
          issueId,
          prUrl: htmlUrl,
          prNumber: pr.number,
          branch: headRef,
          ...(claim
            ? { actorUserId: claim.userId, actorViaAgent: claim.viaAgent }
            : { actorUserId: null }),
        })
      }
      return jsonResponse(200, { ok: true })
    }

    return jsonResponse(200, { ok: true })
  } catch (err) {
    console.error(`[github-webhook] failed:`, err)
    return jsonResponse(500, { error: `webhook handler error` })
  }
}

export const Route = createFileRoute(`/api/webhooks/github`)({
  server: {
    handlers: {
      POST: ({ request }) => handleGithubWebhook(request),
    },
  },
})
