// SES bounce/complaint feedback parsing (EXP-227) — PURE, no DB, no
// transport (unit-tested in email-bounces.test.ts). SES publishes delivery
// feedback to an SNS topic; our webhook (/api/webhooks/ses) receives the SNS
// HTTPS delivery, parses the SES notification here, and records it two ways:
// per-ADDRESS in email_bounces (the admin console's "put this on the SES
// suppression list" worklist) and per-MESSAGE on the matching
// email_deliveries row (status bounced/complained). The DB shell lives in
// the webhook route.

export interface EmailBounceEvent {
  email: string
  kind: `bounce` | `complaint`
  // SES bounceType for bounces (Permanent/Transient/Undetermined); the
  // complaint feedback type (abuse/fraud/…) rides bounceSubType for
  // complaints.
  bounceType: string | null
  bounceSubType: string | null
  diagnostic: string | null
  // SES MessageId of the bounced send — matches
  // email_deliveries.provider_message_id when the send came from our ledgered
  // paths (digest/notification/widget mail; auth mail has no ledger row).
  providerMessageId: string | null
}

// SSRF guard for the SNS SubscriptionConfirmation handshake: the webhook
// blindly fetches SubscribeURL, so only ever fetch a real SNS endpoint.
export function isTrustedSnsUrl(raw: string): boolean {
  let url: URL
  try {
    url = new URL(raw)
  } catch {
    return false
  }
  if (url.protocol !== `https:`) return false
  return /^sns\.[a-z0-9-]+\.amazonaws\.com$/.test(url.hostname)
}

function asString(value: unknown): string | null {
  return typeof value === `string` && value.length > 0 ? value : null
}

function clip(value: string | null, max: number): string | null {
  return value ? value.slice(0, max) : null
}

// Parse one SES notification payload (the JSON inside the SNS envelope's
// `Message`) into zero or more per-recipient events. Unknown/malformed
// shapes and notification types we don't track (Delivery, Send, …) yield [].
export function parseSesNotification(message: unknown): EmailBounceEvent[] {
  if (typeof message !== `object` || message === null) return []
  const msg = message as Record<string, unknown>
  // SES uses `notificationType` for topic notifications and `eventType` for
  // configuration-set event publishing — accept both.
  const type = asString(msg.notificationType) ?? asString(msg.eventType)
  const mail = (msg.mail ?? {}) as Record<string, unknown>
  const providerMessageId = asString(mail.messageId)

  if (type === `Bounce`) {
    const bounce = (msg.bounce ?? {}) as Record<string, unknown>
    const recipients = Array.isArray(bounce.bouncedRecipients)
      ? bounce.bouncedRecipients
      : []
    return recipients.flatMap((r): EmailBounceEvent[] => {
      const rec = (r ?? {}) as Record<string, unknown>
      const email = asString(rec.emailAddress)
      if (!email) return []
      return [
        {
          email: email.trim().toLowerCase(),
          kind: `bounce`,
          bounceType: clip(asString(bounce.bounceType), 32),
          bounceSubType: clip(asString(bounce.bounceSubType), 64),
          diagnostic: clip(asString(rec.diagnosticCode), 1000),
          providerMessageId,
        },
      ]
    })
  }

  if (type === `Complaint`) {
    const complaint = (msg.complaint ?? {}) as Record<string, unknown>
    const recipients = Array.isArray(complaint.complainedRecipients)
      ? complaint.complainedRecipients
      : []
    return recipients.flatMap((r): EmailBounceEvent[] => {
      const rec = (r ?? {}) as Record<string, unknown>
      const email = asString(rec.emailAddress)
      if (!email) return []
      return [
        {
          email: email.trim().toLowerCase(),
          kind: `complaint`,
          bounceType: null,
          bounceSubType: clip(asString(complaint.complaintFeedbackType), 64),
          diagnostic: null,
          providerMessageId,
        },
      ]
    })
  }

  return []
}
