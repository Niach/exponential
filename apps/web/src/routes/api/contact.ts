import { createFileRoute } from "@tanstack/react-router"
import { z } from "zod"
import { isOriginAllowed } from "@/lib/widget/origin"
import {
  corsHeaders,
  jsonResponse,
  preflightResponse,
} from "@/lib/widget/cors"
import {
  clientIpFromRequest,
  envInt,
  TokenBucketLimiter,
} from "@/lib/widget/rate-limit"
import { sendEmail } from "@/lib/email"

// EXP-39: contact/enterprise-inquiry endpoint for the marketing site's form
// (replaces the bare mailto CTA). Mirrors /api/widget/submit's structure —
// keep this route module's import surface SMALL (wide server import graphs
// have failed to register under the nitro-alpha dev server; see
// lib/widget/service.ts).

// Marketing-site origins only. Localhost is allowed outside production for
// the marketing dev server (any port — the pattern grammar ignores ports
// unless one is specified).
const allowedOrigins =
  process.env.NODE_ENV === `production`
    ? [`exponential.at`, `www.exponential.at`]
    : [`exponential.at`, `www.exponential.at`, `localhost`, `127.0.0.1`]

const contactSchema = z.object({
  name: z.string().trim().max(255).optional(),
  email: z.string().trim().email().max(320),
  company: z.string().trim().max(255).optional(),
  message: z.string().trim().min(1).max(5000),
  // Honeypot — the real form never fills this hidden field. Named
  // non-address-like so browser profile autofill can't populate it and
  // silently sink a genuine lead.
  contactNonce: z.string().max(1024).optional(),
  source: z.string().trim().max(100).optional(),
})

const maxContactRequestBytes = 64 * 1024

// Per-IP in-process token bucket, same per-replica stance as the widget,
// plus a global backstop bucket: every accepted request sends an email, so a
// second non-IP limiter bounds total unauthenticated sends even if per-IP
// keying is evaded (cf. publicBoard.createIssue's per-project backstop).
let contactIpLimiter: TokenBucketLimiter | null = null
let contactGlobalLimiter: TokenBucketLimiter | null = null

function getContactLimiters() {
  contactIpLimiter ??= new TokenBucketLimiter({
    capacity: envInt(`CONTACT_RATE_LIMIT_IP_BURST`, 3),
    refillPerHour: envInt(`CONTACT_RATE_LIMIT_PER_IP_HOURLY`, 10),
  })
  contactGlobalLimiter ??= new TokenBucketLimiter({
    capacity: envInt(`CONTACT_RATE_LIMIT_GLOBAL_BURST`, 10),
    refillPerHour: envInt(`CONTACT_RATE_LIMIT_GLOBAL_HOURLY`, 30),
  })
  return { contactIpLimiter, contactGlobalLimiter }
}

// Local copy of email.ts's module-private escaper: the html body interpolates
// untrusted form fields.
function escapeHtml(value: string): string {
  return value
    .replace(/&/g, `&amp;`)
    .replace(/</g, `&lt;`)
    .replace(/>/g, `&gt;`)
    .replace(/"/g, `&quot;`)
    .replace(/'/g, `&#39;`)
}

async function handleContact(request: Request): Promise<Response> {
  const origin = isOriginAllowed(
    request.headers.get(`origin`),
    request.headers.get(`referer`),
    allowedOrigins
  )
  if (!origin.allowed) {
    return jsonResponse(403, { error: `Origin not allowed` })
  }
  const cors = corsHeaders(origin.echoOrigin)

  const contentLength = Number.parseInt(
    request.headers.get(`content-length`) ?? ``,
    10
  )
  if (
    Number.isFinite(contentLength) &&
    contentLength > maxContactRequestBytes
  ) {
    return jsonResponse(413, { error: `Request too large` }, cors)
  }

  let body: unknown
  try {
    body = await request.json()
  } catch {
    return jsonResponse(400, { error: `Expected a JSON body` }, cors)
  }
  const fields = contactSchema.safeParse(body)
  if (!fields.success) {
    return jsonResponse(400, { error: `Invalid contact fields` }, cors)
  }

  const { contactIpLimiter, contactGlobalLimiter } = getContactLimiters()
  // Per-IP bucket first, short-circuiting: a request already throttled by its
  // own IP must not keep draining the shared global bucket — otherwise one
  // hostile IP silences the contact form for everyone.
  const ipLimit = contactIpLimiter.tryTake(
    `ip:${clientIpFromRequest(request)}`
  )
  if (!ipLimit.ok) {
    return jsonResponse(
      429,
      { error: `Too many messages, try again later` },
      { ...cors, "Retry-After": String(ipLimit.retryAfterSeconds) }
    )
  }
  const globalLimit = contactGlobalLimiter.tryTake(`global`)
  if (!globalLimit.ok) {
    return jsonResponse(
      429,
      { error: `Too many messages, try again later` },
      { ...cors, "Retry-After": String(globalLimit.retryAfterSeconds) }
    )
  }

  // Honeypot: pretend success so bots don't adapt; nothing is sent. Warn so
  // an autofill victim whose message vanished is at least diagnosable in logs.
  if (fields.data.contactNonce && fields.data.contactNonce.length > 0) {
    console.warn(
      `contact form honeypot triggered — dropping submission (possible autofill)`
    )
    return jsonResponse(201, { ok: true }, cors)
  }

  const { name, email, company, message, source } = fields.data
  const who = [name, company ? `(${company})` : null]
    .filter(Boolean)
    .join(` `)
  const subject = `Contact form: ${who || email}`

  const rows: [string, string][] = [
    [`Name`, name ?? `—`],
    [`Email`, email],
    [`Company`, company ?? `—`],
    ...(source ? ([[`Source`, source]] as [string, string][]) : []),
  ]
  const rowsHtml = rows
    .map(
      ([label, value]) =>
        `<tr><td style="padding:2px 12px 2px 0;color:#71717a;">${escapeHtml(label)}</td><td style="padding:2px 0;">${escapeHtml(value)}</td></tr>`
    )
    .join(``)
  const html = `<!doctype html>
<html>
  <body style="margin:0;padding:24px;font-family:-apple-system,Segoe UI,Roboto,Helvetica,Arial,sans-serif;color:#18181b;font-size:14px;line-height:1.6;">
    <table style="border-collapse:collapse;">${rowsHtml}</table>
    <pre style="margin:16px 0 0;padding:12px;background:#fafafa;border:1px solid #e4e4e7;border-radius:8px;white-space:pre-wrap;font:inherit;">${escapeHtml(message)}</pre>
  </body>
</html>`
  const text = `${rows.map(([label, value]) => `${label}: ${value}`).join(`\n`)}\n\n${message}`

  try {
    const result = await sendEmail({
      to: process.env.CONTACT_EMAIL_TO ?? `dennis@straehhuber.com`,
      subject,
      html,
      text,
      replyTo: email,
    })
    if (!result.delivered) {
      // No transport configured — tell the form so it can fall back to mailto.
      return jsonResponse(503, { error: `Email is not configured` }, cors)
    }
    return jsonResponse(201, { ok: true }, cors)
  } catch (error) {
    console.error(`contact form send error`, error)
    return jsonResponse(500, { error: `Internal error` }, cors)
  }
}

export const Route = createFileRoute(`/api/contact`)({
  server: {
    handlers: {
      POST: ({ request }) => handleContact(request),
      // JSON POSTs are non-simple (content-type: application/json), so the
      // browser always preflights — OPTIONS must answer with the CORS grant.
      OPTIONS: ({ request }) =>
        preflightResponse(
          isOriginAllowed(
            request.headers.get(`origin`),
            request.headers.get(`referer`),
            allowedOrigins
          ).echoOrigin
        ),
    },
  },
})
