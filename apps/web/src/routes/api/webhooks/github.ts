import { createHmac, timingSafeEqual } from "node:crypto"
import { onForgePullRequest, onForgePush } from "@/lib/ci/triggers"
import { createFileRoute } from "@tanstack/react-router"
import { and, eq, inArray, isNull, or } from "drizzle-orm"
import { db } from "@/db/connection"
import { repositories, sessions } from "@/db/schema"
import { providerConfigWithInstallation, repoInstallationId } from "@/lib/ecode/provider-config"
import { touchSessionActivity } from "@/lib/sessions/watchdog"
import { attributePush, pushedRepositoryIds, pushTime } from "@/lib/sessions/push-attribution"
import { checkFromGithubStatus } from "@/lib/ecode/github-status"
import { isTrainBranch, writeCommitCheck } from "@/lib/merge-train/commit-checks"
import { mergeTrainEdge, onPullRequestPush } from "@/lib/merge-train/worker"
import {
  adoptOutOfBandPullRequest,
  applyPullRequestBase,
  applyPullRequestClosed,
  applyPullRequestDraft,
  applyPullRequestMerged,
  applyPullRequestReopened,
  loadPullRequestByUrl,
} from "@/lib/integrations/pr-sync"
import {
  takePrMergeClaim,
  takePrOpenClaim,
} from "@/lib/integrations/pr-actor-claims"
import {
  checkFromGithubRun,
  pullRequestsForCheck,
  repairPullRequestChecks,
  requiredChecksFor,
  writePullRequestChecks,
} from "@/lib/integrations/pr-checks"
import { resolveRepoInstallationToken } from "@/lib/integrations/github-app"
import {
  resolveAppUserForGithubActor,
  type GithubActorRef,
} from "@/lib/integrations/github-identity"

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

// GitHub webhook receiver — the CLOUD PR-linking + merge-detection trigger
// (self-hosted uses the outbound cron for merges instead). Acts on
// `installation` `created`/`unsuspend` (upsert + clear the suspension mark),
// `suspend` (mark, never delete — REV2-29) and `deleted` (drop the row),
// `installation_repositories` (repo-selection
// changes → flag/heal `repositories.inaccessible_at`) and `pull_request`
// (EXP-1291: the `pull_requests` row by url: merged / closed / reopened /
// base edits / draft flips; `opened` adopts an out-of-band PR whose
// `exp/<IDENTIFIER>` head names an issue) and `push` (EXP-1374: the live
// sessions on that branch show a sign of life; #EXP-1540: the push is
// credited to its session), and EXP-1610's CI events (`check_run` upserts a
// check row; `check_suite`/`workflow_run` completed repair a PR's checks).
// Everything else is acked.
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

    // App lifecycle. Installations are never stored (SLOP-24): discovery lists
    // them LIVE off the member's GitHub token, suspension included. Only the
    // bound repo rows react.
    if (event === `installation`) {
      const payload = JSON.parse(rawBody) as {
        action?: string
        installation?: { id?: number }
      }
      const installation = payload.installation
      if (!installation?.id) {
        return jsonResponse(200, { ok: true })
      }
      if (payload.action === `suspend` || payload.action === `deleted`) {
        // `suspend` is REVERSIBLE (REV2-29) and `deleted` TERMINAL, but both
        // stop token minting: flag every bound repo as inaccessible (the
        // settings badge + the launcher's 412) so the UI stops showing them
        // healthy. `unsuspend` clears nothing — only a VERIFIED access proof
        // clears those flags (the token mint, repositories.list's
        // default-branch heal).
        await db
          .update(repositories)
          .set({ inaccessibleAt: new Date() })
          .where(eq(repoInstallationId(), installation.id))
      }
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
              eq(repoInstallationId(), installation.id),
              inArray(repositories.fullName, removed)
            )
          )
      }
      if (added.length > 0) {
        // Tenant-scoped heal (EXP-363 hardening): a bare full_name match let
        // ANY installation of the App rebind another team's registry row (a
        // renamed-then-squatted account could clear the no-access flag and
        // repoint installation_id from the outside). Heal only rows that are
        // already bound to THIS installation or still unbound (connected
        // before this webhook existed) — a row bound elsewhere is healed by
        // its own token mint, which verifies access.
        await db
          .update(repositories)
          .set({
            inaccessibleAt: null,
            providerConfig: providerConfigWithInstallation(installation.id),
          })
          .where(
            and(
              inArray(repositories.fullName, added),
              or(
                isNull(repoInstallationId()),
                eq(repoInstallationId(), installation.id)
              )
            )
          )
      }
      return jsonResponse(200, { ok: true })
    }

    // EXP-1374 (FEED-141): a push to a live session's branch is its sign of
    // life for the stuck rule (a long gate run pushes, it never messages).
    // Needs the App's `push` event subscription; without it nothing arrives.
    // A branch deletion and a push to the default branch (every merge lands
    // there) vouch for nobody.
    if (event === `push`) {
      const payload = JSON.parse(rawBody) as {
        ref?: string
        after?: string
        deleted?: boolean
        repository?: { full_name?: string; default_branch?: string; pushed_at?: number | string }
        head_commit?: { timestamp?: string } | null
        installation?: { id?: number }
      }
      const branch = payload.ref?.startsWith(`refs/heads/`) ? payload.ref.slice(`refs/heads/`.length) : null
      const fullName = payload.repository?.full_name
      // #EXP-1617: our own CI plans `push` / `tag` / `merge_train` runs.
      if (fullName && payload.ref) {
        onForgePush({ fullName, ref: payload.ref, sha: payload.after ?? null, deleted: payload.deleted === true })
      }
      if (branch && fullName && !payload.deleted && branch !== payload.repository?.default_branch) {
        // #EXP-1540: the same full name may be registered by several teams;
        // only this installation's rows (or still unbound ones) count.
        const installationId = payload.installation?.id ?? null
        await touchSessionActivity(
          and(
            eq(sessions.branch, branch),
            inArray(sessions.repositoryId, pushedRepositoryIds({ fullName, installationId }))
          )!
        )
        // #EXP-1540: and the push is credited to the session that made it,
        // so a PR with no recorded session resolves its author through it.
        // Its time is the push's own (a redelivery is never "now").
        await attributePush({ fullName, branch, installationId, at: pushTime(payload) })
      }
      return jsonResponse(200, { ok: true })
    }

    // EXP-1610: CI. Needs the App's checks:read (+ actions:read) and the
    // check_run / check_suite / workflow_run subscriptions (a human step);
    // without them nothing arrives and every PR stays "no checks known".
    if (event === `check_run`) {
      const payload = JSON.parse(rawBody) as {
        check_run?: Parameters<typeof checkFromGithubRun>[0] & {
          head_sha?: string
          pull_requests?: Array<{ number?: number; head?: { sha?: string } }>
          check_suite?: { head_branch?: string | null }
        }
        repository?: { full_name?: string }
      }
      const run = payload.check_run
      const fullName = payload.repository?.full_name
      const check = run ? checkFromGithubRun(run) : null
      // EXP-1709: a check on a merge train ref is kept per COMMIT.
      if (run?.head_sha && fullName && check && isTrainBranch(run.check_suite?.head_branch)) {
        await writeCommitCheck({ repoFullName: fullName, sha: run.head_sha, check })
        if (check.status === `completed`) mergeTrainEdge({ repoFullName: fullName })
        return jsonResponse(200, { ok: true })
      }
      if (run?.head_sha && fullName && check) {
        const prs = await pullRequestsForCheck({
          repoFullName: fullName,
          numbers: (run.pull_requests ?? []).flatMap((pr) => (typeof pr.number === `number` ? [pr.number] : [])),
          headBranch: run.check_suite?.head_branch ?? null,
        })
        const token = prs.length > 0 ? await resolveRepoInstallationToken(fullName).catch(() => null) : null
        for (const pr of prs) {
          const required = token ? await requiredChecksFor(fullName, pr.base, token) : undefined
          // #FEED-199: GitHub names the PR's current head; a run for a
          // superseded commit (cancelled by a push/rebase) is dropped.
          const prHeadSha = run.pull_requests?.find((item) => item.number === pr.number)?.head?.sha ?? null
          await writePullRequestChecks({
            pullRequestId: pr.id,
            headSha: run.head_sha,
            checks: [check],
            prHeadSha,
            ...(required ? { required } : {}),
          })
        }
        // EXP-1709: a PR's own checks feed its merge readiness.
        if (prs.length > 0 && check.status === `completed`) mergeTrainEdge({ repoFullName: fullName })
      }
      return jsonResponse(200, { ok: true })
    }

    // EXP-1709: a commit STATUS (a required context may be one) on a merge
    // train ref is kept per commit like a check run.
    if (event === `status`) {
      const payload = JSON.parse(rawBody) as {
        sha?: string
        context?: string
        state?: string
        target_url?: string | null
        branches?: Array<{ name?: string }>
        repository?: { full_name?: string }
      }
      const fullName = payload.repository?.full_name
      if (payload.sha && payload.context && fullName && (payload.branches ?? []).some((b) => isTrainBranch(b.name))) {
        await writeCommitCheck({
          repoFullName: fullName,
          sha: payload.sha,
          check: { name: payload.context, ...checkFromGithubStatus(payload.state), detailsUrl: payload.target_url ?? null },
        })
        mergeTrainEdge({ repoFullName: fullName })
      }
      return jsonResponse(200, { ok: true })
    }

    // A finished suite / workflow run: one repair read of its PRs' checks
    // heals any check_run delivery that was lost (throttled per PR).
    if (event === `check_suite` || event === `workflow_run`) {
      const payload = JSON.parse(rawBody) as {
        action?: string
        check_suite?: { head_branch?: string | null; head_sha?: string | null; pull_requests?: Array<{ number?: number }> }
        workflow_run?: { head_branch?: string | null; head_sha?: string | null; pull_requests?: Array<{ number?: number }> }
        repository?: { full_name?: string }
      }
      const suite = payload.check_suite ?? payload.workflow_run
      const fullName = payload.repository?.full_name
      if (payload.action === `completed` && suite && fullName) {
        const prs = await pullRequestsForCheck({
          repoFullName: fullName,
          numbers: (suite.pull_requests ?? []).flatMap((pr) => (typeof pr.number === `number` ? [pr.number] : [])),
          headBranch: suite.head_branch ?? null,
        })
        // Read at the suite's own commit, never whatever the branch has moved to.
        for (const pr of prs) await repairPullRequestChecks(pr.id, Date.now(), { headSha: suite.head_sha ?? null })
      }
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
        title?: string
        // EXP-1706: its `Fixes #IDENT` lines link an out-of-band PR.
        body?: string | null
        draft?: boolean
        // When GitHub last changed the PR: a generic draft sync never lets
        // an older delivery overwrite a newer state.
        updated_at?: string
        head?: { ref?: string; sha?: string; repo?: { full_name?: string } | null }
        // The base GitHub reports on every delivery.
        base?: { ref?: string; repo?: { full_name?: string } | null }
        // EXP-617: the GitHub identity behind the event. `user` is the PR
        // author, `merged_by` whoever pressed Merge; both are the App bot for
        // anything our own server did, which is exactly why they complement
        // the actor claims instead of competing with them.
        user?: GithubActorRef
        merged_by?: GithubActorRef | null
      }
      repository?: { full_name?: string }
      sender?: GithubActorRef
      // `edited` names what changed; a base change moves the stored base.
      changes?: { base?: { from?: { ref?: string } } }
    }

    const pr = payload.pull_request
    if (!pr?.html_url) {
      return jsonResponse(200, { ok: true })
    }
    const htmlUrl = pr.html_url
    const repoFullName = payload.repository?.full_name
    const headRef = pr.head?.ref
    const row = await loadPullRequestByUrl(htmlUrl)

    // Merge: the claim inside applyPullRequestMerged makes a redelivery (or
    // the in-app merge that already wrote it) a no-op.
    if (payload.action === `closed` && pr.merged === true) {
      // REV-26: a PR whose `opened` delivery was lost is adopted here first
      // (branch parse), so its issue never stays unlinked.
      let merged = row
      if (!merged && repoFullName && headRef && pr.number) {
        await adoptOutOfBandPullRequest({
          repoFullName,
          url: htmlUrl,
          number: pr.number,
          title: pr.title ?? null,
          body: pr.body ?? null,
          head: headRef,
          base: pr.base?.ref ?? ``,
          actorUserId: null,
        })
        merged = await loadPullRequestByUrl(htmlUrl)
      }
      if (!merged) return jsonResponse(200, { ok: true })
      // EXP-494: an in-app merge claimed its actor right before the GitHub
      // call (the webhook beats its own write).
      const claim =
        repoFullName && pr.number ? takePrMergeClaim(repoFullName, pr.number) : null
      // EXP-617: whoever pressed Merge, if they ever connected that GitHub
      // account here. Never `pull_request.user` (an author wants to hear it).
      const githubActorUserId = await resolveAppUserForGithubActor(
        pr.merged_by ?? payload.sender
      )
      await applyPullRequestMerged({
        pullRequestId: merged.id,
        mergedAt: pr.merged_at ? new Date(pr.merged_at) : new Date(),
        // The base GitHub merged it into: a retarget whose `edited` delivery
        // is late or lost must not leave the PR held by a stale stack parent.
        ...(pr.base?.ref ? { base: pr.base.ref } : {}),
        githubActorUserId,
        ...(claim
          ? { actorUserId: claim.userId, actorViaAgent: claim.viaAgent }
          : { actorUserId: null }),
      })
      return jsonResponse(200, { ok: true })
    }

    if (payload.action === `closed`) {
      if (row) await applyPullRequestClosed({ pullRequestId: row.id })
      return jsonResponse(200, { ok: true })
    }

    // FEED-160: GitHub's own clock for this delivery's PR state; stored as
    // the row's `provider_updated_at`, the only thing a late delivery is
    // compared with (our `updated_at` moves on any unrelated write).
    const parsedAt = pr.updated_at ? new Date(pr.updated_at) : null
    const changedAt = parsedAt && !Number.isNaN(parsedAt.getTime()) ? parsedAt : null

    // #EXP-1617: our own CI on a new or moved head. A head from another
    // repository (a fork, or a deleted one) never runs.
    if (
      (payload.action === `opened` || payload.action === `synchronize` || payload.action === `reopened`) &&
      repoFullName &&
      pr.number &&
      pr.head?.sha &&
      pr.base?.ref
    ) {
      const headRepo = pr.head.repo?.full_name ?? null
      onForgePullRequest({
        fullName: repoFullName,
        number: pr.number,
        headSha: pr.head.sha,
        headRef: pr.head.ref ?? null,
        base: pr.base.ref,
        fork: headRepo === null || headRepo.toLowerCase() !== (pr.base.repo?.full_name ?? repoFullName).toLowerCase(),
      })
    }

    if (payload.action === `reopened`) {
      if (row) {
        await applyPullRequestReopened({
          pullRequestId: row.id,
          base: pr.base?.ref ?? null,
          draft: pr.draft === true,
          ...(changedAt ? { providerUpdatedAt: changedAt } : {}),
        })
      }
      return jsonResponse(200, { ok: true })
    }

    // EXP-1418: every other delivery carries GitHub's draft flag; a live
    // row follows it (a PR drafted or readied while a delivery was lost,
    // our own pr_open's row, an `edited`/`synchronize`). Ready again = a
    // merge-train edge: the PR may be ready already, waiting on it.
    const draft =
      payload.action === `converted_to_draft`
        ? true
        : payload.action === `ready_for_review`
          ? false
          : typeof pr.draft === `boolean`
            ? pr.draft
            : null
    if (row && draft !== null) {
      const explicit = payload.action === `converted_to_draft` || payload.action === `ready_for_review`
      const flipped = await applyPullRequestDraft({
        pullRequestId: row.id,
        draft,
        // The flag riding any other delivery: only when that delivery is
        // not older than the last PR state we applied (a late `synchronize`
        // must not undo a newer `ready_for_review`). An explicit flip always
        // applies and records its time.
        ...(changedAt ? (explicit ? { providerUpdatedAt: changedAt } : { notBefore: changedAt }) : {}),
      })
      // A draft readied is a merge-train edge (readiness re-reads it).
      if (flipped && !draft) mergeTrainEdge({ pullRequestId: row.id })
      if (explicit) {
        return jsonResponse(200, { ok: true })
      }
    }

    if (payload.action === `opened` && repoFullName && headRef && pr.number) {
      // EXP-494: pr_open claimed its actor keyed on the head branch.
      const claim = takePrOpenClaim(repoFullName, headRef)
      // EXP-617: for a PR opened out of band this is the only signal that
      // names a human; our own pr_open calls carry the App bot.
      const githubActorUserId = await resolveAppUserForGithubActor(
        pr.user ?? payload.sender
      )
      await adoptOutOfBandPullRequest({
        repoFullName,
        url: htmlUrl,
        number: pr.number,
        title: pr.title ?? null,
        body: pr.body ?? null,
        head: headRef,
        base: pr.base?.ref ?? ``,
        draft: pr.draft === true,
        githubActorUserId,
        ...(claim
          ? { actorUserId: claim.userId, actorViaAgent: claim.viaAgent }
          : { actorUserId: null }),
      })
      return jsonResponse(200, { ok: true })
    }

    // Retargeted (on github.com, by our own retarget, or GitHub's restack).
    if (payload.action === `edited` && payload.changes?.base && row && pr.base?.ref) {
      await applyPullRequestBase({ pullRequestId: row.id, base: pr.base.ref })
      return jsonResponse(200, { ok: true })
    }

    // Batch 3: a new head (a push, a rebase) after a green run left the
    // state `success` at the OLD head, which the watchdog sweep never
    // re-reads: read the new head now so its runs (pending) are swept.
    if (payload.action === `synchronize` && row && pr.head?.sha) {
      // EXP-1709: a new head = merge-train `push` (checks again; a testing
      // unit leaves its run).
      await onPullRequestPush(row.id)
      await repairPullRequestChecks(row.id, Date.now(), { headSha: pr.head.sha })
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
