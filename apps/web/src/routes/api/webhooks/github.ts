import { createHmac, timingSafeEqual } from "node:crypto"
import { createFileRoute } from "@tanstack/react-router"
import { and, eq, inArray } from "drizzle-orm"
import { db } from "@/db/connection"
import { githubInstallations, issues, repositories } from "@/db/schema"
import {
  applyPrClosedState,
  applyPrMergeState,
  applyPrOpenedState,
  applyPrReopenedState,
  findIssueIdByBranch,
} from "@/lib/integrations/pr-sync"
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

// GitHub webhook receiver — the CLOUD PR-linking + merge-detection trigger
// (self-hosted uses the outbound cron for merges instead). Acts on
// `installation` `created`/`unsuspend`/`deleted`/`suspend` (keep
// github_installations in sync), `installation_repositories` (repo-selection
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
        installation?: {
          id?: number
          account?: { login?: string; type?: string }
        }
      }
      const installation = payload.installation
      if (!installation?.id) {
        return jsonResponse(200, { ok: true })
      }
      if (payload.action === `created` || payload.action === `unsuspend`) {
        await db
          .insert(githubInstallations)
          .values({
            installationId: installation.id,
            accountLogin: installation.account?.login ?? null,
            accountType: installation.account?.type ?? null,
          })
          .onConflictDoUpdate({
            target: githubInstallations.installationId,
            set: {
              accountLogin: installation.account?.login ?? null,
              accountType: installation.account?.type ?? null,
              updatedAt: new Date(),
            },
          })
      } else if (
        payload.action === `deleted` ||
        payload.action === `suspend`
      ) {
        // A suspended installation can't mint tokens: flag every repo bound to
        // it as inaccessible (the settings badge + the launcher's 412), then
        // drop the row — its team links CASCADE away with it. `unsuspend`
        // re-inserts above; the flag heals on the next successful mint/list.
        // Invalidate the repo cache BEFORE the delete: the invalidation
        // resolves the linked teams through the installation links,
        // which cascade away with the row.
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
        repositories_added?: Array<{ full_name?: string }>
        repositories_removed?: Array<{ full_name?: string }>
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
      }
      if (added.length > 0) {
        await db
          .update(repositories)
          .set({ inaccessibleAt: null, installationId: installation.id })
          .where(inArray(repositories.fullName, added))
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
      }
      repository?: { full_name?: string }
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
      for (const issueId of issueIds) {
        await applyPrMergeState({
          issueId,
          prUrl: htmlUrl,
          mergedAt,
          actorUserId: null,
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
      return jsonResponse(200, { ok: true })
    }

    // Opened out-of-band: link the PR to its issue if it has none yet.
    if (payload.action === `opened` && repoFullName && headRef && pr.number) {
      const issueIds = await resolveIssuesForPr({
        htmlUrl,
        repoFullName,
        headRef,
      })
      for (const issueId of issueIds) {
        await applyPrOpenedState({
          issueId,
          prUrl: htmlUrl,
          prNumber: pr.number,
          branch: headRef,
          actorUserId: null,
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
