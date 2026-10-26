import { createHmac, timingSafeEqual } from "node:crypto"
import { createFileRoute } from "@tanstack/react-router"
import { and, eq, inArray, isNull, or } from "drizzle-orm"
import { db } from "@/db/connection"
import { repositories, sessions } from "@/db/schema"
import { touchSessionActivity } from "@/lib/sessions/watchdog"
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
// sessions on that branch show a sign of life). Everything else is acked.
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
          .where(eq(repositories.installationId, installation.id))
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
              eq(repositories.installationId, installation.id),
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
          .set({ inaccessibleAt: null, installationId: installation.id })
          .where(
            and(
              inArray(repositories.fullName, added),
              or(
                isNull(repositories.installationId),
                eq(repositories.installationId, installation.id)
              )
            )
          )
      }
      return jsonResponse(200, { ok: true })
    }

    // EXP-1374 (FEED-141): a push to a live session's branch is its sign of
    // life for the stuck rule (a long gate run pushes, it never messages).
    // Needs the App's `push` event subscription; without it nothing arrives.
    if (event === `push`) {
      const payload = JSON.parse(rawBody) as { ref?: string; repository?: { full_name?: string } }
      const branch = payload.ref?.startsWith(`refs/heads/`) ? payload.ref.slice(`refs/heads/`.length) : null
      const fullName = payload.repository?.full_name
      if (branch && fullName) {
        await touchSessionActivity(
          and(
            eq(sessions.branch, branch),
            inArray(
              sessions.repositoryId,
              db.select({ id: repositories.id }).from(repositories).where(eq(repositories.fullName, fullName))
            )
          )!
        )
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
        draft?: boolean
        head?: { ref?: string }
        // The base GitHub reports on every delivery.
        base?: { ref?: string }
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

    if (payload.action === `reopened`) {
      if (row) {
        await applyPullRequestReopened({
          pullRequestId: row.id,
          base: pr.base?.ref ?? null,
          draft: pr.draft === true,
        })
      }
      return jsonResponse(200, { ok: true })
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

    if (
      (payload.action === `converted_to_draft` || payload.action === `ready_for_review`) &&
      row
    ) {
      await applyPullRequestDraft({
        pullRequestId: row.id,
        draft: payload.action === `converted_to_draft`,
      })
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
