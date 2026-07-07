// Transactional email — the SINGLE sender for the whole app. Two transports:
//
//   1. Resend (cloud): RESEND_API_KEY set → plain fetch, no SDK.
//   2. SMTP (self-host): SMTP_HOST set → nodemailer (lazy-imported so the
//      module graph stays light when SMTP is unused).
//
// Resend wins when both are configured. With neither, every send is a logged
// no-op so auth + notification flows degrade gracefully instead of throwing;
// `emailEnabled` lets the UI hide email-dependent affordances (forgot-password,
// the email-notification prefs panel) on such instances.

import type { Transporter } from "nodemailer"

const RESEND_ENDPOINT = `https://api.resend.com/emails`

export type EmailProvider = `resend` | `smtp`

export type EmailSendResult = {
  // false only when no transport is configured (the logged no-op).
  delivered: boolean
  provider: EmailProvider | null
  messageId: string | null
}

export const emailEnabled = Boolean(
  process.env.RESEND_API_KEY || process.env.SMTP_HOST
)

function fromAddress(): string {
  return process.env.EMAIL_FROM ?? `Exponential <noreply@exponential.at>`
}

// Lazy nodemailer transporter singleton (SMTP self-host path). The runtime
// import stays dynamic so nodemailer is only loaded when SMTP is configured.
let smtpTransporter: Transporter | null = null

async function getSmtpTransporter() {
  if (smtpTransporter) return smtpTransporter
  const nodemailer = await import(`nodemailer`)
  const port = Number(process.env.SMTP_PORT ?? 587)
  smtpTransporter = nodemailer.createTransport({
    host: process.env.SMTP_HOST,
    port,
    // Implicit TLS for 465 unless overridden; STARTTLS otherwise.
    secure: process.env.SMTP_SECURE
      ? process.env.SMTP_SECURE === `true`
      : port === 465,
    auth: process.env.SMTP_USER
      ? {
          user: process.env.SMTP_USER,
          pass: process.env.SMTP_PASS,
        }
      : undefined,
  })
  return smtpTransporter
}

export async function sendEmail(args: {
  to: string
  subject: string
  html: string
  text: string
  headers?: Record<string, string>
}): Promise<EmailSendResult> {
  const apiKey = process.env.RESEND_API_KEY

  if (apiKey) {
    const res = await fetch(RESEND_ENDPOINT, {
      method: `POST`,
      headers: {
        Authorization: `Bearer ${apiKey}`,
        "Content-Type": `application/json`,
      },
      body: JSON.stringify({
        from: fromAddress(),
        to: [args.to],
        subject: args.subject,
        html: args.html,
        text: args.text,
        ...(args.headers ? { headers: args.headers } : {}),
      }),
    })
    if (!res.ok) {
      const body = await res.text().catch(() => ``)
      throw new Error(`Resend ${res.status}: ${body.slice(0, 300)}`)
    }
    const json = (await res.json().catch(() => null)) as { id?: string } | null
    return {
      delivered: true,
      provider: `resend`,
      messageId: json?.id ?? null,
    }
  }

  if (process.env.SMTP_HOST) {
    const transporter = await getSmtpTransporter()
    const info = await transporter.sendMail({
      from: fromAddress(),
      to: args.to,
      subject: args.subject,
      html: args.html,
      text: args.text,
      headers: args.headers,
    })
    return {
      delivered: true,
      provider: `smtp`,
      messageId: (info as { messageId?: string }).messageId ?? null,
    }
  }

  process.stderr.write(
    `[email] no transport configured (RESEND_API_KEY / SMTP_HOST unset) — dropping "${args.subject}" to ${args.to}\n`
  )
  return { delivered: false, provider: null, messageId: null }
}

// Headings/bodies interpolate user content (issue titles, comment previews) —
// escape them so a title like `<img onerror=…>` can't inject markup into the
// rendered email.
function escapeHtml(value: string): string {
  return value
    .replace(/&/g, `&amp;`)
    .replace(/</g, `&lt;`)
    .replace(/>/g, `&gt;`)
    .replace(/"/g, `&quot;`)
    .replace(/'/g, `&#39;`)
}

// Minimal dark-friendly template shared by the auth + notification emails: a
// heading, one line of context, a single action button (plus the raw link for
// clients that strip styles), and an optional unsubscribe footer.
function actionEmailHtml(args: {
  heading: string
  body: string
  actionLabel: string
  actionUrl: string
  unsubscribeUrl?: string
}): string {
  const unsubscribeFooter = args.unsubscribeUrl
    ? `
      <p style="margin:16px 0 0;font-size:12px;line-height:1.6;color:#a1a1aa;">
        You receive these emails because notifications are enabled for your account.
        <a href="${args.unsubscribeUrl}" style="color:#71717a;">Unsubscribe from email notifications</a>
      </p>`
    : ``
  return `<!doctype html>
<html>
  <body style="margin:0;padding:32px 16px;background:#fafafa;font-family:-apple-system,Segoe UI,Roboto,Helvetica,Arial,sans-serif;color:#18181b;">
    <div style="max-width:480px;margin:0 auto;background:#ffffff;border:1px solid #e4e4e7;border-radius:12px;padding:32px;">
      <h1 style="margin:0 0 12px;font-size:18px;">${escapeHtml(args.heading)}</h1>
      <p style="margin:0 0 24px;font-size:14px;line-height:1.6;color:#3f3f46;">${escapeHtml(args.body)}</p>
      <a href="${args.actionUrl}"
         style="display:inline-block;background:#18181b;color:#ffffff;text-decoration:none;font-size:14px;font-weight:600;padding:10px 20px;border-radius:8px;">
        ${args.actionLabel}
      </a>
      <p style="margin:24px 0 0;font-size:12px;line-height:1.6;color:#a1a1aa;">
        If the button doesn't work, copy this link into your browser:<br/>
        <a href="${args.actionUrl}" style="color:#71717a;word-break:break-all;">${args.actionUrl}</a>
      </p>${unsubscribeFooter}
    </div>
  </body>
</html>`
}

export async function sendPasswordResetEmail(args: {
  to: string
  url: string
}): Promise<void> {
  await sendEmail({
    to: args.to,
    subject: `Reset your Exponential password`,
    html: actionEmailHtml({
      heading: `Reset your password`,
      body: `Someone requested a password reset for this email address. If that was you, set a new password below — the link expires in one hour. If not, you can safely ignore this email.`,
      actionLabel: `Set a new password`,
      actionUrl: args.url,
    }),
    text: `Reset your Exponential password: ${args.url}\n\nIf you didn't request this, ignore this email.`,
  })
}

export async function sendVerificationEmail(args: {
  to: string
  url: string
}): Promise<void> {
  await sendEmail({
    to: args.to,
    subject: `Verify your email for Exponential`,
    html: actionEmailHtml({
      heading: `Verify your email`,
      body: `Confirm this email address to finish setting up your Exponential account.`,
      actionLabel: `Verify email`,
      actionUrl: args.url,
    }),
    text: `Verify your email for Exponential: ${args.url}`,
  })
}

// One bundled item of the push-first digest email. `url` deep-links to the
// item's issue (null for notifications without one).
export interface DigestEmailItem {
  title: string
  body: string | null
  url: string | null
}

// The email leg of the push-first notification pipeline (item q): push fires
// immediately on create; anything still unread ~1h later lands here — ONE
// email per user bundling every pending notification, with per-item deep
// links and a one-click unsubscribe (RFC 8058 headers).
export async function sendNotificationDigestEmail(args: {
  to: string
  items: DigestEmailItem[]
  appUrl: string
  unsubscribeUrl: string
}): Promise<EmailSendResult> {
  const count = args.items.length
  const subject =
    count === 1
      ? `1 unread notification in Exponential`
      : `${count} unread notifications in Exponential`

  const itemsHtml = args.items
    .map((item) => {
      const title = escapeHtml(item.title)
      const titleHtml = item.url
        ? `<a href="${item.url}" style="color:#18181b;font-weight:600;text-decoration:none;">${title}</a>`
        : `<span style="font-weight:600;">${title}</span>`
      const bodyHtml = item.body
        ? `<div style="margin-top:2px;font-size:13px;line-height:1.5;color:#71717a;">${escapeHtml(item.body)}</div>`
        : ``
      return `<div style="padding:12px 0;border-bottom:1px solid #f4f4f5;font-size:14px;line-height:1.5;">${titleHtml}${bodyHtml}</div>`
    })
    .join(``)

  const html = `<!doctype html>
<html>
  <body style="margin:0;padding:32px 16px;background:#fafafa;font-family:-apple-system,Segoe UI,Roboto,Helvetica,Arial,sans-serif;color:#18181b;">
    <div style="max-width:480px;margin:0 auto;background:#ffffff;border:1px solid #e4e4e7;border-radius:12px;padding:32px;">
      <h1 style="margin:0 0 4px;font-size:18px;">${escapeHtml(subject)}</h1>
      <p style="margin:0 0 8px;font-size:13px;line-height:1.6;color:#71717a;">A catch-up on what you haven't seen yet.</p>
      ${itemsHtml}
      <a href="${args.appUrl}"
         style="display:inline-block;margin-top:20px;background:#18181b;color:#ffffff;text-decoration:none;font-size:14px;font-weight:600;padding:10px 20px;border-radius:8px;">
        Open Exponential
      </a>
      <p style="margin:16px 0 0;font-size:12px;line-height:1.6;color:#a1a1aa;">
        You receive these emails because notifications are enabled for your account.
        <a href="${args.unsubscribeUrl}" style="color:#71717a;">Unsubscribe from email notifications</a>
      </p>
    </div>
  </body>
</html>`

  const textItems = args.items
    .map((item) => `- ${item.title}${item.url ? ` (${item.url})` : ``}`)
    .join(`\n`)

  return await sendEmail({
    to: args.to,
    subject,
    html,
    text: `${subject}\n\n${textItems}\n\nOpen Exponential: ${args.appUrl}\n\nUnsubscribe from email notifications: ${args.unsubscribeUrl}`,
    headers: {
      "List-Unsubscribe": `<${args.unsubscribeUrl}>`,
      "List-Unsubscribe-Post": `List-Unsubscribe=One-Click`,
    },
  })
}

// One-way helpdesk resolution notice for an external widget reporter. CLEAN
// reporter-facing copy only — no assignee names, no page/UA metadata, none of
// the internal buildWidgetDescription block, no workspace context, no links
// into the app (reporters have no account).
export async function sendReporterResolutionEmail(args: {
  to: string
  issueTitle: string
}): Promise<EmailSendResult> {
  const heading = `Your report has been resolved`
  const body = `Your report "${args.issueTitle}" has been resolved. Thanks for the feedback!`
  return await sendEmail({
    to: args.to,
    subject: `Your report "${args.issueTitle}" has been resolved`,
    html: `<!doctype html>
<html>
  <body style="margin:0;padding:32px 16px;background:#fafafa;font-family:-apple-system,Segoe UI,Roboto,Helvetica,Arial,sans-serif;color:#18181b;">
    <div style="max-width:480px;margin:0 auto;background:#ffffff;border:1px solid #e4e4e7;border-radius:12px;padding:32px;">
      <h1 style="margin:0 0 12px;font-size:18px;">${escapeHtml(heading)}</h1>
      <p style="margin:0;font-size:14px;line-height:1.6;color:#3f3f46;">${escapeHtml(body)}</p>
    </div>
  </body>
</html>`,
    text: body,
  })
}
