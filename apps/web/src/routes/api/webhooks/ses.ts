import { timingSafeEqual } from "node:crypto"
import { createFileRoute } from "@tanstack/react-router"
import { eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { emailBounces, emailDeliveries, users } from "@/db/schema"
import { updateEmailPrefs } from "@/lib/notification-prefs"
import {
  isTrustedSnsUrl,
  parseSesNotification,
  type EmailBounceEvent,
} from "@/lib/email-bounces"

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": `application/json` },
  })
}

// Constant-time secret comparison (mirrors the GitHub webhook's signature
// check; timingSafeEqual throws on unequal-length buffers).
function secretMatches(provided: string, expected: string): boolean {
  const a = Buffer.from(provided)
  const b = Buffer.from(expected)
  if (a.length !== b.length) return false
  return timingSafeEqual(a, b)
}

// Upsert the per-address email_bounces rows and stamp the originating
// email_deliveries rows (matched by SES MessageId) bounced/complained.
async function recordEmailBounceEvents(
  events: EmailBounceEvent[],
  now: Date = new Date()
): Promise<void> {
  for (const event of events) {
    await db
      .insert(emailBounces)
      .values({
        email: event.email,
        kind: event.kind,
        bounceType: event.bounceType,
        bounceSubType: event.bounceSubType,
        diagnostic: event.diagnostic,
        eventCount: 1,
        lastEventAt: now,
      })
      .onConflictDoUpdate({
        target: emailBounces.email,
        set: {
          kind: event.kind,
          bounceType: event.bounceType,
          bounceSubType: event.bounceSubType,
          diagnostic: event.diagnostic,
          eventCount: sql`${emailBounces.eventCount} + 1`,
          lastEventAt: now,
          updatedAt: now,
        },
      })
    if (event.providerMessageId) {
      await db
        .update(emailDeliveries)
        .set({
          status: event.kind === `complaint` ? `complained` : `bounced`,
          error: event.diagnostic,
          updatedAt: now,
        })
        .where(eq(emailDeliveries.providerMessageId, event.providerMessageId))
    }
    // A spam complaint is a stronger signal than any preference: immediately
    // disable ALL notification email for the complaining account, without
    // waiting for an operator. Idempotent; the user can re-enable in account
    // settings. (Address-level suppression in sendEmail additionally blocks
    // every stream, so re-enabling only matters after the bounce row is
    // cleared by an admin.)
    if (event.kind === `complaint`) {
      const matched = await db
        .select({ id: users.id })
        .from(users)
        .where(eq(sql`lower(${users.email})`, event.email.trim().toLowerCase()))
      for (const user of matched) {
        await updateEmailPrefs(user.id, { emailEnabled: false })
      }
      if (matched.length > 0) {
        console.log(
          `[ses-webhook] complaint: disabled notification email for ${matched.length} account(s)`
        )
      }
    }
  }
}

// SES delivery-feedback receiver (EXP-227): SES → SNS topic → HTTPS
// subscription to this route. Records bounces/complaints per address
// (email_bounces — the admin console's suppression worklist) and per message
// (email_deliveries status). Setup (AWS console): SES → verified identity →
// Notifications → attach an SNS topic for Bounce + Complaint feedback, then
// subscribe `${BETTER_AUTH_URL}/api/webhooks/ses?secret=${SES_WEBHOOK_SECRET}`
// via HTTPS — the SubscriptionConfirmation handshake below auto-confirms.
//
// Auth is the shared secret in the query string rather than full SNS
// signature verification (cert fetch + canonical string): the endpoint only
// ever RECORDS bounce facts for the admin console, and the secret-bearing
// URL is known only to AWS and us.
async function handleSesWebhook(request: Request): Promise<Response> {
  try {
    const secret = process.env.SES_WEBHOOK_SECRET
    if (!secret) {
      return jsonResponse(503, { error: `webhook not configured` })
    }
    const provided = new URL(request.url).searchParams.get(`secret`)
    if (!provided || !secretMatches(provided, secret)) {
      return jsonResponse(401, { error: `invalid secret` })
    }

    // SNS HTTPS deliveries are JSON with a text/plain content type; the
    // message type rides a header (fall back to the envelope's Type field).
    const rawBody = await request.text()
    let envelope: Record<string, unknown>
    try {
      envelope = JSON.parse(rawBody) as Record<string, unknown>
    } catch {
      return jsonResponse(400, { error: `invalid JSON` })
    }
    const type =
      request.headers.get(`x-amz-sns-message-type`) ??
      (typeof envelope.Type === `string` ? envelope.Type : null)

    if (type === `SubscriptionConfirmation`) {
      const subscribeUrl =
        typeof envelope.SubscribeURL === `string` ? envelope.SubscribeURL : ``
      if (!isTrustedSnsUrl(subscribeUrl)) {
        return jsonResponse(400, { error: `untrusted SubscribeURL` })
      }
      await fetch(subscribeUrl)
      return jsonResponse(200, { ok: true })
    }

    if (type === `Notification`) {
      let message: unknown = null
      try {
        message =
          typeof envelope.Message === `string`
            ? JSON.parse(envelope.Message)
            : null
      } catch {
        // Non-JSON Message (e.g. a raw test publish) — ack and ignore.
      }
      const events = parseSesNotification(message)
      if (events.length > 0) {
        await recordEmailBounceEvents(events)
        console.log(
          `[ses-webhook] recorded ${events.length} bounce/complaint event(s)`
        )
      }
      return jsonResponse(200, { ok: true })
    }

    // UnsubscribeConfirmation and anything else: ack and ignore.
    return jsonResponse(200, { ok: true })
  } catch (err) {
    console.error(`[ses-webhook] failed:`, err)
    return jsonResponse(500, { error: `webhook handler error` })
  }
}

export const Route = createFileRoute(`/api/webhooks/ses`)({
  server: {
    handlers: {
      POST: ({ request }) => handleSesWebhook(request),
    },
  },
})
