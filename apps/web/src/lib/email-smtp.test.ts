// The SMTP (self-host) leg of sendEmail through the REAL nodemailer: only
// createTransport is wrapped — it records the config sendEmail derives from
// env and swaps the network SMTP transport for nodemailer's own stream
// transport, so the asserted message is the exact RFC 822 output nodemailer
// would hand an SMTP server. Guards nodemailer major bumps (9 → 10).
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

const captured = vi.hoisted(() => ({
  configs: [] as unknown[],
  raw: [] as string[],
  // Set to make the transport's send reject (an SMTP server refusing).
  failWith: null as Error | null,
}))

vi.mock(`@/db/connection`, () => {
  function chain(): Promise<unknown[]> & Record<string, () => unknown> {
    const p = Promise.resolve([] as unknown[]) as Promise<unknown[]> &
      Record<string, () => unknown>
    for (const m of [`from`, `where`, `limit`]) {
      p[m] = () => p
    }
    return p
  }
  return { db: { select: () => chain() } }
})

vi.mock(`nodemailer`, async (importOriginal) => {
  const real = await importOriginal<typeof import("nodemailer")>()
  const createTransport = (config: unknown) => {
    captured.configs.push(config)
    const transporter = real.createTransport({
      streamTransport: true,
      buffer: true,
      newline: `unix`,
    })
    const send = transporter.sendMail.bind(transporter)
    transporter.sendMail = (async (mail: Parameters<typeof send>[0]) => {
      if (captured.failWith) throw captured.failWith
      const info = await send(mail)
      captured.raw.push(info.message.toString())
      return info
    }) as typeof transporter.sendMail
    return transporter
  }
  return {
    ...real,
    createTransport,
    default: { ...real.default, createTransport },
  }
})

const SMTP_ENV = [
  `AWS_SES_REGION`,
  `SMTP_HOST`,
  `SMTP_PORT`,
  `SMTP_SECURE`,
  `SMTP_USER`,
  `SMTP_PASS`,
  `EMAIL_FROM`,
  `EMAIL_REPLY_TO`,
]

async function importEmail(env: Record<string, string>) {
  vi.resetModules()
  // email.ts reads `?? default`, so unset vars must be ABSENT, not empty.
  for (const key of SMTP_ENV) vi.stubEnv(key, undefined)
  for (const [key, value] of Object.entries(env)) vi.stubEnv(key, value)
  return await import(`@/lib/email`)
}

beforeEach(() => {
  captured.configs.length = 0
  captured.raw.length = 0
  captured.failWith = null
})

afterEach(() => {
  vi.unstubAllEnvs()
})

function splitMessage(raw: string) {
  const at = raw.indexOf(`\n\n`)
  return { head: raw.slice(0, at), body: raw.slice(at + 2) }
}

describe(`sendEmail over SMTP (real nodemailer, stream transport)`, () => {
  it(`sends the exact message: headers, from, to, reply-to, subject, both bodies`, async () => {
    const email = await importEmail({
      SMTP_HOST: `mail.example.com`,
      EMAIL_FROM: `Exponential <noreply@example.com>`,
      EMAIL_REPLY_TO: `support@example.com`,
    })

    const result = await email.sendEmail({
      to: `user@example.com`,
      subject: `Hello from SMTP`,
      html: `<p>Hi <strong>there</strong></p>`,
      text: `Hi there`,
      headers: {
        "List-Unsubscribe": `<https://app.example.com/u>`,
        "List-Unsubscribe-Post": `List-Unsubscribe=One-Click`,
      },
    })

    // Default port 587 = STARTTLS, no auth without SMTP_USER.
    expect(captured.configs).toEqual([
      { host: `mail.example.com`, port: 587, secure: false, auth: undefined },
    ])
    expect(result.delivered).toBe(true)
    expect(result.provider).toBe(`smtp`)
    expect(result.subject).toBe(`Hello from SMTP`)
    expect(result.messageId).toMatch(/^<.+@.+>$/)

    expect(captured.raw).toHaveLength(1)
    const { head, body } = splitMessage(captured.raw[0]!)
    expect(head).toContain(`From: Exponential <noreply@example.com>`)
    expect(head).toContain(`To: user@example.com`)
    expect(head).toContain(`Reply-To: support@example.com`)
    expect(head).toContain(`Subject: Hello from SMTP`)
    expect(head).toContain(`List-Unsubscribe: <https://app.example.com/u>`)
    expect(head).toContain(`List-Unsubscribe-Post: List-Unsubscribe=One-Click`)
    expect(head).toContain(`Message-ID: ${result.messageId}`)
    expect(head).toContain(`MIME-Version: 1.0`)
    expect(head).toMatch(/Content-Type: multipart\/alternative;/)
    expect(body).toMatch(
      /Content-Type: text\/plain; charset=utf-8[\s\S]*\n\nHi there\n/
    )
    expect(body).toMatch(
      /Content-Type: text\/html; charset=utf-8[\s\S]*\n\n<p>Hi <strong>there<\/strong><\/p>\n/
    )
    // Plain part precedes the html part (clients render the last they support).
    expect(body.indexOf(`text/plain`)).toBeLessThan(body.indexOf(`text/html`))
  })

  it(`uses the default from-address, implicit TLS on 465, SMTP auth from env, and an explicit replyTo over EMAIL_REPLY_TO`, async () => {
    const email = await importEmail({
      SMTP_HOST: `smtp.example.org`,
      SMTP_PORT: `465`,
      SMTP_USER: `mailer`,
      SMTP_PASS: `s3cret`,
      EMAIL_REPLY_TO: `support@example.com`,
    })

    await email.sendEmail({
      to: `a@example.org`,
      subject: `Second`,
      html: `<p>x</p>`,
      text: `x`,
      replyTo: `owner@example.org`,
    })

    expect(captured.configs).toEqual([
      {
        host: `smtp.example.org`,
        port: 465,
        secure: true,
        auth: { user: `mailer`, pass: `s3cret` },
      },
    ])
    const { head } = splitMessage(captured.raw[0]!)
    expect(head).toContain(`From: Exponential <noreply@exponential.at>`)
    expect(head).toContain(`Reply-To: owner@example.org`)
    expect(head).not.toContain(`support@example.com`)
  })

  it(`honours SMTP_SECURE over the port default and reuses one transporter`, async () => {
    const email = await importEmail({
      SMTP_HOST: `smtp.example.org`,
      SMTP_PORT: `465`,
      SMTP_SECURE: `false`,
    })
    const mail = { to: `a@example.org`, subject: `s`, html: `<p>x</p>`, text: `x` }
    await email.sendEmail(mail)
    await email.sendEmail(mail)

    expect(captured.configs).toEqual([
      { host: `smtp.example.org`, port: 465, secure: false, auth: undefined },
    ])
    expect(captured.raw).toHaveLength(2)
  })

  it(`propagates a transport failure to the caller (no swallowing)`, async () => {
    const email = await importEmail({ SMTP_HOST: `smtp.example.org` })
    captured.failWith = new Error(`550 mailbox unavailable`)
    await expect(
      email.sendEmail({
        to: `a@example.org`,
        subject: `s`,
        html: `<p>x</p>`,
        text: `x`,
      })
    ).rejects.toThrow(`550 mailbox unavailable`)
  })
})
