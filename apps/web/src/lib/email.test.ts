// Transport-level tests for the single email sender. The transports are
// mocked (fetch for Resend); the graceful no-op contract is exercised with no
// transport configured — nothing here needs a network or an SMTP server.
import { afterEach, describe, expect, it, vi } from "vitest"

async function importEmail(env: Record<string, string>) {
  vi.resetModules()
  vi.stubEnv(`RESEND_API_KEY`, ``)
  vi.stubEnv(`SMTP_HOST`, ``)
  for (const [key, value] of Object.entries(env)) {
    vi.stubEnv(key, value)
  }
  return await import(`@/lib/email`)
}

afterEach(() => {
  vi.unstubAllEnvs()
  vi.unstubAllGlobals()
})

describe(`emailEnabled`, () => {
  it(`is false with neither RESEND_API_KEY nor SMTP_HOST`, async () => {
    const email = await importEmail({})
    expect(email.emailEnabled).toBe(false)
  })

  it(`is true with RESEND_API_KEY`, async () => {
    const email = await importEmail({ RESEND_API_KEY: `re_test` })
    expect(email.emailEnabled).toBe(true)
  })

  it(`is true with SMTP_HOST (self-host)`, async () => {
    const email = await importEmail({ SMTP_HOST: `mail.example.com` })
    expect(email.emailEnabled).toBe(true)
  })
})

describe(`sendEmail with no transport`, () => {
  it(`is a logged no-op that never throws`, async () => {
    const email = await importEmail({})
    const stderrSpy = vi
      .spyOn(process.stderr, `write`)
      .mockImplementation(() => true)

    const result = await email.sendEmail({
      to: `user@example.com`,
      subject: `Hello`,
      html: `<p>hi</p>`,
      text: `hi`,
    })

    expect(result).toEqual({ delivered: false, provider: null, messageId: null })
    expect(stderrSpy).toHaveBeenCalled()
    stderrSpy.mockRestore()
  })

  it(`notification + reporter emails degrade the same way (self-host §6.6)`, async () => {
    const email = await importEmail({})
    const stderrSpy = vi
      .spyOn(process.stderr, `write`)
      .mockImplementation(() => true)

    await expect(
      email.sendNotificationEmail({
        to: `user@example.com`,
        subject: `s`,
        heading: `h`,
        body: `b`,
        actionUrl: `https://app.example.com/w/a/projects/b/issues/A-1`,
        unsubscribeUrl: `https://app.example.com/api/email/unsubscribe?token=t`,
      })
    ).resolves.toMatchObject({ delivered: false })

    await expect(
      email.sendReporterResolutionEmail({
        to: `reporter@example.com`,
        issueTitle: `Broken button`,
      })
    ).resolves.toMatchObject({ delivered: false })

    stderrSpy.mockRestore()
  })
})

describe(`sendNotificationEmail over the Resend transport (mocked)`, () => {
  it(`sends the deep link, unsubscribe footer, and one-click headers`, async () => {
    const email = await importEmail({ RESEND_API_KEY: `re_test` })
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ id: `msg_1` }), { status: 200 })
    )
    vi.stubGlobal(`fetch`, fetchMock)

    const result = await email.sendNotificationEmail({
      to: `user@example.com`,
      subject: `Dana assigned you MET-12`,
      heading: `Dana assigned you MET-12`,
      body: `Fix the login flow`,
      actionUrl: `https://app.example.com/w/metric/projects/web/issues/MET-12`,
      unsubscribeUrl: `https://app.example.com/api/email/unsubscribe?token=tok-1`,
    })

    expect(result).toEqual({
      delivered: true,
      provider: `resend`,
      messageId: `msg_1`,
    })
    expect(fetchMock).toHaveBeenCalledTimes(1)
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit]
    expect(url).toBe(`https://api.resend.com/emails`)
    const payload = JSON.parse(init.body as string) as {
      to: string[]
      subject: string
      html: string
      text: string
      headers: Record<string, string>
    }
    expect(payload.to).toEqual([`user@example.com`])
    expect(payload.headers[`List-Unsubscribe`]).toBe(
      `<https://app.example.com/api/email/unsubscribe?token=tok-1>`
    )
    expect(payload.headers[`List-Unsubscribe-Post`]).toBe(
      `List-Unsubscribe=One-Click`
    )
    expect(payload.html).toContain(
      `https://app.example.com/w/metric/projects/web/issues/MET-12`
    )
    expect(payload.html).toContain(
      `https://app.example.com/api/email/unsubscribe?token=tok-1`
    )
    expect(payload.text).toContain(`Unsubscribe`)
  })

  it(`escapes user content in the rendered html`, async () => {
    const email = await importEmail({ RESEND_API_KEY: `re_test` })
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ id: `msg_2` }), { status: 200 })
    )
    vi.stubGlobal(`fetch`, fetchMock)

    await email.sendNotificationEmail({
      to: `user@example.com`,
      subject: `x`,
      heading: `<script>alert(1)</script>`,
      body: `a & b <img>`,
      actionUrl: `https://app.example.com/x`,
      unsubscribeUrl: `https://app.example.com/u`,
    })

    const [, init] = fetchMock.mock.calls[0] as [string, RequestInit]
    const payload = JSON.parse(init.body as string) as { html: string }
    expect(payload.html).not.toContain(`<script>`)
    expect(payload.html).toContain(`&lt;script&gt;`)
    expect(payload.html).toContain(`a &amp; b &lt;img&gt;`)
  })

  it(`keeps the reporter resolution email clean (no app links, no metadata)`, async () => {
    const email = await importEmail({ RESEND_API_KEY: `re_test` })
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ id: `msg_3` }), { status: 200 })
    )
    vi.stubGlobal(`fetch`, fetchMock)

    await email.sendReporterResolutionEmail({
      to: `reporter@example.com`,
      issueTitle: `Broken button`,
    })

    const [, init] = fetchMock.mock.calls[0] as [string, RequestInit]
    const payload = JSON.parse(init.body as string) as {
      subject: string
      html: string
      text: string
    }
    expect(payload.subject).toContain(`Broken button`)
    expect(payload.text).toContain(`has been resolved`)
    // Clean template: no deep links into the workspace, no unsubscribe pref
    // link (reporters have no account), no metadata block.
    expect(payload.html).not.toContain(`/w/`)
    expect(payload.html).not.toContain(`unsubscribe`)
    expect(payload.html).not.toContain(`User agent`)
  })
})
