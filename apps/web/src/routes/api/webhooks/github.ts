import { createHmac, timingSafeEqual } from "node:crypto"
import { createFileRoute } from "@tanstack/react-router"
import { eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { githubInstallations, issues } from "@/db/schema"
import {
  applyPrMergeState,
  applyPrOpenedState,
  findIssueIdByBranch,
} from "@/lib/integrations/pr-sync"

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

// Resolve the payload's PR to one of our issues: exact prUrl match first, then
// a deterministic fallback that parses the `exp/<IDENTIFIER>` head branch and
// resolves through the repositories registry — so PRs opened out-of-band still
// link.
async function resolveIssueForPr(args: {
  htmlUrl: string
  repoFullName?: string
  headRef?: string
}): Promise<string | null> {
  const [byUrl] = await db
    .select({ id: issues.id })
    .from(issues)
    .where(eq(issues.prUrl, args.htmlUrl))
    .limit(1)
  if (byUrl) return byUrl.id
  if (args.repoFullName && args.headRef) {
    return findIssueIdByBranch(args.repoFullName, args.headRef)
  }
  return null
}

// GitHub webhook receiver — the CLOUD PR-linking + merge-detection trigger
// (self-hosted uses the outbound cron for merges instead). Acts on
// `installation` `created`/`unsuspend`/`deleted`/`suspend` (keep
// github_installations in sync for the UI "installed" state), `pull_request`
// `opened` (link an out-of-band PR to its issue) and `closed`+`merged` (flip
// prState). Issues
// resolve by exact prUrl OR by the `exp/<IDENTIFIER>` head-branch parse;
// everything else is acked and ignored.
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
        // A suspended installation can't mint tokens, so treat it like a
        // removal: drop the row and stop reporting "installed". `unsuspend`
        // re-inserts it above.
        await db
          .delete(githubInstallations)
          .where(eq(githubInstallations.installationId, installation.id))
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

    // Merge: flip prState → merged, stamp prMergedAt, emit pr_merged (once).
    if (payload.action === `closed` && pr.merged === true) {
      const issueId = await resolveIssueForPr({ htmlUrl, repoFullName, headRef })
      if (!issueId) return jsonResponse(200, { ok: true })
      const mergedAt = pr.merged_at ? new Date(pr.merged_at) : new Date()
      await applyPrMergeState({
        issueId,
        prUrl: htmlUrl,
        mergedAt,
        actorUserId: null,
      })
      return jsonResponse(200, { ok: true })
    }

    // Opened out-of-band: link the PR to its issue if it has none yet.
    if (payload.action === `opened` && repoFullName && headRef && pr.number) {
      const issueId = await resolveIssueForPr({ htmlUrl, repoFullName, headRef })
      if (issueId) {
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
