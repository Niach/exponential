// Transactional email via Resend (https://resend.com). Plain fetch — no SDK
// dependency. When RESEND_API_KEY is unset (e.g. self-hosted without email),
// every send is a logged no-op so auth flows degrade gracefully instead of
// throwing; `emailEnabled` lets the UI hide email-dependent affordances
// (forgot-password) on such instances.

const RESEND_ENDPOINT = `https://api.resend.com/emails`

export const emailEnabled = Boolean(process.env.RESEND_API_KEY)

function fromAddress(): string {
  return process.env.EMAIL_FROM ?? `Exponential <noreply@exponential.at>`
}

export async function sendEmail(args: {
  to: string
  subject: string
  html: string
  text: string
}): Promise<void> {
  const apiKey = process.env.RESEND_API_KEY
  if (!apiKey) {
    process.stderr.write(
      `[email] RESEND_API_KEY unset — dropping "${args.subject}" to ${args.to}\n`
    )
    return
  }
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
    }),
  })
  if (!res.ok) {
    const body = await res.text().catch(() => ``)
    throw new Error(`Resend ${res.status}: ${body.slice(0, 300)}`)
  }
}

// Minimal dark-friendly template shared by the auth emails: a heading, one
// line of context, and a single action button (plus the raw link for clients
// that strip styles).
function actionEmailHtml(args: {
  heading: string
  body: string
  actionLabel: string
  actionUrl: string
}): string {
  return `<!doctype html>
<html>
  <body style="margin:0;padding:32px 16px;background:#fafafa;font-family:-apple-system,Segoe UI,Roboto,Helvetica,Arial,sans-serif;color:#18181b;">
    <div style="max-width:480px;margin:0 auto;background:#ffffff;border:1px solid #e4e4e7;border-radius:12px;padding:32px;">
      <h1 style="margin:0 0 12px;font-size:18px;">${args.heading}</h1>
      <p style="margin:0 0 24px;font-size:14px;line-height:1.6;color:#3f3f46;">${args.body}</p>
      <a href="${args.actionUrl}"
         style="display:inline-block;background:#18181b;color:#ffffff;text-decoration:none;font-size:14px;font-weight:600;padding:10px 20px;border-radius:8px;">
        ${args.actionLabel}
      </a>
      <p style="margin:24px 0 0;font-size:12px;line-height:1.6;color:#a1a1aa;">
        If the button doesn't work, copy this link into your browser:<br/>
        <a href="${args.actionUrl}" style="color:#71717a;word-break:break-all;">${args.actionUrl}</a>
      </p>
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
