// Transport-level tests for the single email sender. The transports are
// mocked (the SESv2 SDK module for Amazon SES); the graceful no-op contract is
// exercised with no transport configured — nothing here needs a network or an
// SMTP server.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

const { sesSendMock } = vi.hoisted(() => ({ sesSendMock: vi.fn() }))

vi.mock(`@aws-sdk/client-sesv2`, () => {
  class SESv2Client {
    send = sesSendMock
  }
  class SendEmailCommand {
    input: unknown
    constructor(input: unknown) {
      this.input = input
    }
  }
  return { SESv2Client, SendEmailCommand }
})

// The mocked SendEmailCommand's captured input.
type SesInput = {
  FromEmailAddress: string
  Destination: { ToAddresses: string[] }
  ReplyToAddresses?: string[]
  Content: {
    Simple: {
      Subject: { Data: string }
      Body: { Html: { Data: string }; Text: { Data: string } }
      Headers?: Array<{ Name: string; Value: string }>
    }
  }
}

function sesCallInput(call = 0): SesInput {
  const command = sesSendMock.mock.calls[call]?.[0] as { input: SesInput }
  return command.input
}

async function importEmail(env: Record<string, string>) {
  vi.resetModules()
  vi.stubEnv(`AWS_SES_REGION`, ``)
  vi.stubEnv(`SMTP_HOST`, ``)
  for (const [key, value] of Object.entries(env)) {
    vi.stubEnv(key, value)
  }
  return await import(`@/lib/email`)
}

beforeEach(() => {
  sesSendMock.mockReset()
  sesSendMock.mockResolvedValue({ MessageId: `msg_1` })
})

afterEach(() => {
  vi.unstubAllEnvs()
})

describe(`emailEnabled`, () => {
  it(`is false with neither AWS_SES_REGION nor SMTP_HOST`, async () => {
    const email = await importEmail({})
    expect(email.emailEnabled).toBe(false)
  })

  it(`is true with AWS_SES_REGION`, async () => {
    const email = await importEmail({ AWS_SES_REGION: `eu-central-1` })
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
    expect(sesSendMock).not.toHaveBeenCalled()
    stderrSpy.mockRestore()
  })

  it(`digest + reporter emails degrade the same way (self-host §6.6)`, async () => {
    const email = await importEmail({})
    const stderrSpy = vi
      .spyOn(process.stderr, `write`)
      .mockImplementation(() => true)

    await expect(
      email.sendNotificationDigestEmail({
        to: `user@example.com`,
        items: [{ title: `t`, body: `b`, url: null }],
        appUrl: `https://app.example.com`,
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

describe(`sendEmail over the SES transport (mocked)`, () => {
  it(`maps replyTo to top-level ReplyToAddresses, never a custom header (SES rejects Reply-To there)`, async () => {
    const email = await importEmail({ AWS_SES_REGION: `eu-central-1` })

    await email.sendEmail({
      to: `dennis@example.com`,
      subject: `Contact form`,
      html: `<p>hi</p>`,
      text: `hi`,
      replyTo: `visitor@example.com`,
    })

    const input = sesCallInput()
    expect(input.ReplyToAddresses).toEqual([`visitor@example.com`])
    expect(input.Content.Simple.Headers ?? []).not.toContainEqual(
      expect.objectContaining({ Name: `Reply-To` })
    )
  })
})

describe(`sendNotificationDigestEmail over the SES transport (mocked)`, () => {
  it(`bundles the items with per-item deep links, unsubscribe footer, and one-click headers`, async () => {
    const email = await importEmail({ AWS_SES_REGION: `eu-central-1` })

    const result = await email.sendNotificationDigestEmail({
      to: `user@example.com`,
      items: [
        {
          title: `Dana assigned you MET-12`,
          body: `Fix the login flow`,
          url: `https://app.example.com/w/metric/projects/web/issues/MET-12`,
        },
        {
          title: `Dana commented on MET-9`,
          body: null,
          url: `https://app.example.com/w/metric/projects/web/issues/MET-9`,
        },
      ],
      appUrl: `https://app.example.com`,
      unsubscribeUrl: `https://app.example.com/api/email/unsubscribe?token=tok-1`,
    })

    expect(result).toEqual({
      delivered: true,
      provider: `ses`,
      messageId: `msg_1`,
    })
    expect(sesSendMock).toHaveBeenCalledTimes(1)
    const input = sesCallInput()
    expect(input.Destination.ToAddresses).toEqual([`user@example.com`])
    expect(input.Content.Simple.Subject.Data).toBe(
      `2 unread notifications in Exponential`
    )
    expect(input.Content.Simple.Headers).toContainEqual({
      Name: `List-Unsubscribe`,
      Value: `<https://app.example.com/api/email/unsubscribe?token=tok-1>`,
    })
    expect(input.Content.Simple.Headers).toContainEqual({
      Name: `List-Unsubscribe-Post`,
      Value: `List-Unsubscribe=One-Click`,
    })
    const html = input.Content.Simple.Body.Html.Data
    expect(html).toContain(
      `https://app.example.com/w/metric/projects/web/issues/MET-12`
    )
    expect(html).toContain(
      `https://app.example.com/w/metric/projects/web/issues/MET-9`
    )
    expect(html).toContain(
      `https://app.example.com/api/email/unsubscribe?token=tok-1`
    )
    const text = input.Content.Simple.Body.Text.Data
    expect(text).toContain(`Dana assigned you MET-12`)
    expect(text).toContain(`Unsubscribe`)
  })

  it(`uses a singular subject for a single pending notification`, async () => {
    const email = await importEmail({ AWS_SES_REGION: `eu-central-1` })

    await email.sendNotificationDigestEmail({
      to: `user@example.com`,
      items: [{ title: `t`, body: null, url: null }],
      appUrl: `https://app.example.com`,
      unsubscribeUrl: `https://app.example.com/u`,
    })

    expect(sesCallInput().Content.Simple.Subject.Data).toBe(
      `1 unread notification in Exponential`
    )
  })

  it(`escapes user content in the rendered html`, async () => {
    const email = await importEmail({ AWS_SES_REGION: `eu-central-1` })

    await email.sendNotificationDigestEmail({
      to: `user@example.com`,
      items: [
        {
          title: `<script>alert(1)</script>`,
          body: `a & b <img>`,
          url: null,
        },
      ],
      appUrl: `https://app.example.com`,
      unsubscribeUrl: `https://app.example.com/u`,
    })

    const html = sesCallInput().Content.Simple.Body.Html.Data
    expect(html).not.toContain(`<script>`)
    expect(html).toContain(`&lt;script&gt;`)
    expect(html).toContain(`a &amp; b &lt;img&gt;`)
  })

  it(`keeps the reporter resolution email clean (no app links, no metadata)`, async () => {
    const email = await importEmail({ AWS_SES_REGION: `eu-central-1` })

    await email.sendReporterResolutionEmail({
      to: `reporter@example.com`,
      issueTitle: `Broken button`,
    })

    const input = sesCallInput()
    expect(input.Content.Simple.Subject.Data).toContain(`Broken button`)
    expect(input.Content.Simple.Body.Text.Data).toContain(`has been resolved`)
    // Clean template: no deep links into the workspace, no unsubscribe pref
    // link (reporters have no account), no metadata block.
    const html = input.Content.Simple.Body.Html.Data
    expect(html).not.toContain(`/w/`)
    expect(html).not.toContain(`unsubscribe`)
    expect(html).not.toContain(`User agent`)
  })
})
