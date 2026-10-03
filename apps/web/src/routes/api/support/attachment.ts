import { createFileRoute } from "@tanstack/react-router"
import { and, eq, isNull } from "drizzle-orm"
import { db } from "@/db/connection"
import { attachments } from "@/db/schema"
import { jsonResponse } from "@/lib/widget/cors"
import { clientIpFromRequest } from "@/lib/widget/rate-limit"
import {
  findIssueByReporterToken,
  getReporterRateLimiters,
} from "@/lib/reporter/service"
import { getObject, toResponseBody } from "@/lib/storage"
import { isAcceptedImageContentType } from "@/lib/storage/issue-attachments"

// SLOP-4: the reporter page's picture bytes. Same posture as thread.ts —
// POST with the token in the JSON body (the page loads each picture into
// an object URL, so a GET with the token in the query string would land
// the credential in proxy logs and browser history). Serves ONLY what
// thread.ts lists: the issue's attachments with no uploader (the
// reporter's own screenshot + pictures) of an image type — never a
// member's upload, never a comment attachment. Private, uncached.
const UUID_RE =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i

async function handleAttachmentRead(request: Request): Promise<Response> {
  const { readLimiter } = getReporterRateLimiters()
  const limit = readLimiter.tryTake(`ip:${clientIpFromRequest(request)}`)
  if (!limit.ok) {
    return jsonResponse(
      429,
      { error: `Too many requests, try again later` },
      { "Retry-After": String(limit.retryAfterSeconds) }
    )
  }

  let token: unknown
  let id: unknown
  try {
    const body = (await request.json()) as { token?: unknown; id?: unknown }
    token = body.token
    id = body.id
  } catch {
    return jsonResponse(400, { error: `Expected a JSON body` })
  }
  if (typeof token !== `string`) {
    return jsonResponse(400, { error: `Missing token` })
  }
  if (typeof id !== `string` || !UUID_RE.test(id)) {
    return jsonResponse(400, { error: `Missing attachment id` })
  }

  const resolved = await findIssueByReporterToken(token)
  if (!resolved) {
    return jsonResponse(404, { error: `Conversation not found` })
  }

  const [attachment] = await db
    .select({
      storageKey: attachments.storageKey,
      contentType: attachments.contentType,
      sizeBytes: attachments.sizeBytes,
    })
    .from(attachments)
    .where(
      and(
        eq(attachments.id, id),
        eq(attachments.issueId, resolved.issue.id),
        isNull(attachments.uploaderId),
        isNull(attachments.commentId)
      )
    )
    .limit(1)
  if (!attachment || !isAcceptedImageContentType(attachment.contentType)) {
    // Indistinguishable from an unknown token: nothing about other
    // attachments leaks through the status code.
    return jsonResponse(404, { error: `Conversation not found` })
  }

  const object = await getObject(attachment.storageKey)
  const body = object ? await toResponseBody(object.Body) : null
  if (!object || !body) {
    return jsonResponse(404, { error: `Conversation not found` })
  }

  const headers = new Headers({
    "Content-Type": attachment.contentType,
    "Content-Disposition": `inline`,
    "Cache-Control": `private, no-store`,
    "X-Content-Type-Options": `nosniff`,
  })
  if (typeof object.ContentLength === `number`) {
    headers.set(`Content-Length`, object.ContentLength.toString())
  } else {
    headers.set(`Content-Length`, attachment.sizeBytes.toString())
  }
  return new Response(body, { headers })
}

export const Route = createFileRoute(`/api/support/attachment`)({
  server: {
    handlers: {
      POST: ({ request }) => handleAttachmentRead(request),
    },
  },
})
