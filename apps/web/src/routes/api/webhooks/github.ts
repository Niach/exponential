import { createHmac, timingSafeEqual } from "node:crypto"
import { createFileRoute } from "@tanstack/react-router"
import { eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { issues } from "@/db/schema"
import { applyPrMergeState } from "@/lib/integrations/pr-sync"

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

// GitHub webhook receiver — the CLOUD merge-detection trigger (self-hosted uses
// the outbound cron instead). Only acts on `pull_request` closed+merged events,
// matched to an issue by exact pr_url; everything else is acked and ignored.
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
    if (event !== `pull_request`) {
      return jsonResponse(200, { ok: true })
    }

    const payload = JSON.parse(rawBody) as {
      action?: string
      pull_request?: {
        html_url?: string
        merged?: boolean
        merged_at?: string | null
      }
    }

    // Ack-and-ignore anything that isn't a merge.
    if (
      payload.action !== `closed` ||
      payload.pull_request?.merged !== true ||
      !payload.pull_request.html_url
    ) {
      return jsonResponse(200, { ok: true })
    }

    const htmlUrl = payload.pull_request.html_url
    const [issue] = await db
      .select({ id: issues.id })
      .from(issues)
      .where(eq(issues.prUrl, htmlUrl))
      .limit(1)

    // Not one of our PRs.
    if (!issue) {
      return jsonResponse(200, { ok: true })
    }

    const mergedAt = payload.pull_request.merged_at
      ? new Date(payload.pull_request.merged_at)
      : new Date()

    await applyPrMergeState({
      issueId: issue.id,
      prUrl: htmlUrl,
      mergedAt,
      actorUserId: null,
    })

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
