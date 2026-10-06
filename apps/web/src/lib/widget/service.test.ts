import { beforeEach, describe, expect, it, vi } from "vitest"

// Locks EXP-53 + EXP-50 on the widget submit path: after the transaction
// commits, every human team member is notified via
// fireAndForgetNewIssueNotify (issue_created); in a solo team the issue
// is inserted with the sole human member as assignee (subscribed as
// `assignee`, NO assignment notification — issue_created already covers it).

const h = vi.hoisted(() => ({
  getSoleHumanMemberId: vi.fn(async (): Promise<string | null> => null),
  ensureSubscribed: vi.fn(),
  fireAndForgetNewIssueNotify: vi.fn(),
  fireAndForgetAssignmentNotify: vi.fn(),
  // REV-25: the config endpoint's per-IP bucket.
  configIpTryTake: vi.fn(
    (): { ok: true } | { ok: false; retryAfterSeconds: number } => ({
      ok: true,
    })
  ),
  // SLOP-4: the reporter's confirmation email (carries the magic link).
  sendReporterConfirmationEmail: vi.fn(async () => ({
    delivered: true,
    provider: `ses`,
    messageId: `msg-1`,
    subject: `Acme: we got your report`,
  })),
  emailEnabled: { value: true },
  inserts: [] as Array<{ table: unknown; values: Record<string, unknown> }>,
  // Post-commit inserts (the email-delivery ledger) go through db.insert.
  dbInserts: [] as Array<{ table: unknown; values: Record<string, unknown> }>,
  // Rows the in-tx / db label selects resolve (EXP-435 reporter labels).
  labelSelectRows: [] as Array<Record<string, unknown>>,
  txShouldFail: false,
}))

const tx = {
  insert: (table: unknown) => ({
    values: (values: Record<string, unknown>) => {
      h.inserts.push({ table, values })
      return {
        returning: async () => [
          { id: values.id ?? `generated`, identifier: `EXP-9`, ...values },
        ],
        // Awaited without .returning() (subscribers/submissions inserts).
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        then: (res: any, rej: any) => Promise.resolve().then(res, rej),
        onConflictDoNothing: () => ({
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          then: (res: any, rej: any) => Promise.resolve().then(res, rej),
        }),
      }
    },
  }),
  // The EXP-435 label re-select inside the submit transaction.
  select: () => ({
    from: () => ({
      where: () => Promise.resolve(h.labelSelectRows),
    }),
  }),
}

vi.mock(`@/db/connection`, () => ({
  db: {
    transaction: vi.fn(async (fn: (t: typeof tx) => Promise<unknown>) => {
      if (h.txShouldFail) throw new Error(`TX_FAILED`)
      return fn(tx)
    }),
    insert: (table: unknown) => ({
      values: (values: Record<string, unknown>) => {
        h.dbInserts.push({ table, values })
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        return {
          then: (res: any, rej: any) => Promise.resolve().then(res, rej),
        }
      },
    }),
    // resolveWidgetConfigLabels' ordered lookup (EXP-435).
    select: () => ({
      from: () => ({
        where: () => ({
          orderBy: () => Promise.resolve(h.labelSelectRows),
        }),
      }),
    }),
  },
}))

// lib/trpc.ts imports `auth`/db at module scope; only generateTxId is used.
vi.mock(`@/lib/trpc`, () => ({ generateTxId: vi.fn(async () => 1) }))
vi.mock(`@/lib/billing`, () => ({
  assertWithinStorageLimit: vi.fn(async () => undefined),
}))
// SLOP-4: the reporter conversation bits — the magic link is a recompute
// over the issue id, so the URL the email carries is predictable here.
vi.mock(`@/lib/reporter/service`, () => ({
  MAX_REPORTER_MESSAGE_CHARS: 10_000,
  reporterConversationUrl: (token: string) =>
    `https://app.test/support/${token}`,
}))
vi.mock(`@/lib/reporter/token`, () => ({
  mintReporterToken: (issueId: string) => `${issueId}.mac`,
}))
vi.mock(`@/lib/email-enabled`, () => ({
  get emailEnabled() {
    return h.emailEnabled.value
  },
}))
vi.mock(`@/lib/email`, () => ({
  sendReporterConfirmationEmail: h.sendReporterConfirmationEmail,
  deliveryStatus: (result: { delivered: boolean; suppressed?: boolean }) =>
    result.delivered ? `sent` : result.suppressed ? `suppressed` : `failed`,
}))
vi.mock(`@/lib/storage/issue-attachments`, () => ({
  buildAttachmentStorageKey: vi.fn(
    (_issueId: string, attachmentId: string) => `key-${attachmentId}`
  ),
  buildAttachmentUrl: vi.fn(() => `/api/attachments/x`),
  isAcceptedImageContentType: (contentType: string) =>
    [`image/png`, `image/jpeg`, `image/webp`, `image/gif`, `image/avif`].includes(
      contentType
    ),
  maxImageUploadBytes: 10 * 1024 * 1024,
  sanitizeUploadFilename: vi.fn(
    (name: string, fallback: string) => name || fallback
  ),
}))
vi.mock(`@/lib/storage/image-dimensions`, () => ({
  getImageDimensions: vi.fn(() => null),
}))
vi.mock(`@/lib/storage`, () => ({
  uploadObject: vi.fn(async () => undefined),
  deleteObject: vi.fn(async () => undefined),
}))
vi.mock(`@/lib/team-membership`, () => ({
  getSoleHumanMemberId: h.getSoleHumanMemberId,
}))
vi.mock(`@/lib/integrations/subscriptions`, () => ({
  ensureSubscribed: h.ensureSubscribed,
}))
vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetNewIssueNotify: h.fireAndForgetNewIssueNotify,
  fireAndForgetAssignmentNotify: h.fireAndForgetAssignmentNotify,
}))
vi.mock(`@/lib/widget/rate-limit`, () => ({
  clientIpFromRequest: () => `203.0.113.7`,
  getWidgetConfigIpLimiter: () => ({ tryTake: h.configIpTryTake }),
}))

import {
  attachments,
  emailDeliveries,
  issueLabels,
  issues,
  issueSubscribers,
  widgetSubmissions,
} from "@/db/schema"
import { uploadObject, deleteObject } from "@/lib/storage"
import {
  createWidgetSubmission,
  defaultWidgetLauncher,
  handleWidgetConfig,
  normalizedWidgetFormToggles,
  resolveWidgetConfigLabels,
  sanitizeWidgetCustomFields,
  sanitizeWidgetHexColor,
  sanitizeWidgetLabelIds,
  sanitizeWidgetLauncher,
  sanitizeWidgetTheme,
  WidgetRequestError,
  type WidgetConfigWithBoard,
} from "@/lib/widget/service"
import { defaultLauncher as widgetPackageDefaultLauncher } from "@exp/widget/launcher"

const config = {
  id: `cfg-1`,
  teamId: `ws-1`,
  boardId: `proj-1`,
  publicKey: `expw_test`,
  enabled: true,
  allowedDomains: [`example.com`],
  formConfig: null,
  boardSlug: `board`,
  boardName: `Board`,
  boardDeletedAt: null,
  teamSlug: `acme`,
  teamName: `Acme`,
} as unknown as WidgetConfigWithBoard

function submitForm(): FormData {
  const form = new FormData()
  form.set(`title`, `Button broken`)
  return form
}

const issueInsert = () => h.inserts.find((i) => i.table === issues)

// FEED-5: reporter-attached pictures — per-image validation, one attachment
// row per image, and multi-key S3 rollback when the transaction fails.
describe(`createWidgetSubmission attached pictures`, () => {
  beforeEach(() => {
    h.inserts.length = 0
    h.txShouldFail = false
    h.getSoleHumanMemberId.mockResolvedValue(null)
    vi.mocked(uploadObject).mockClear()
    vi.mocked(deleteObject).mockClear()
  })

  // happy-dom's File lacks arrayBuffer(); the service needs it for the S3 body.
  const makeFile = (name: string, type: string, size: number) => {
    const file = new File([new Uint8Array(size)], name, { type })
    if (typeof file.arrayBuffer !== `function`) {
      Object.defineProperty(file, `arrayBuffer`, {
        value: async () => new Uint8Array(size).buffer,
      })
    }
    return file
  }

  const withImages = (count: number, type = `image/png`, size = 10) => {
    const form = submitForm()
    form.set(`screenshot`, makeFile(`shot.png`, `image/png`, 16))
    for (let index = 0; index < count; index += 1) {
      form.append(`images`, makeFile(`pic-${index}.png`, type, size))
    }
    return form
  }

  it(`stores one attachment row per image and embeds them in the description`, async () => {
    await createWidgetSubmission({
      config,
      formData: withImages(2),
      userAgent: null,
    })
    const rows = h.inserts.filter((insert) => insert.table === attachments)
    expect(rows).toHaveLength(3)
    expect(rows.map((row) => row.values.filename)).toEqual([
      `shot.png`,
      `pic-0.png`,
      `pic-1.png`,
    ])
    expect(vi.mocked(uploadObject)).toHaveBeenCalledTimes(3)
    const description = issueInsert()!.values.description as string
    expect(description.match(/!\[Screenshot\]/g)).toHaveLength(1)
    expect(description.match(/!\[Image\]/g)).toHaveLength(2)
  })

  it(`rejects more than 3 images`, async () => {
    await expect(
      createWidgetSubmission({
        config,
        formData: withImages(4),
        userAgent: null,
      })
    ).rejects.toMatchObject({ status: 400, message: `Too many images` })
  })

  it(`rejects an unsupported image type`, async () => {
    await expect(
      createWidgetSubmission({
        config,
        formData: withImages(1, `application/pdf`),
        userAgent: null,
      })
    ).rejects.toMatchObject({ status: 400, message: `Unsupported image type` })
  })

  it(`rejects an over-limit image`, async () => {
    await expect(
      createWidgetSubmission({
        config,
        formData: withImages(1, `image/png`, 10 * 1024 * 1024 + 1),
        userAgent: null,
      })
    ).rejects.toMatchObject({ status: 413, message: `Image too large` })
  })

  it(`reclaims every uploaded object when the transaction fails`, async () => {
    h.txShouldFail = true
    await expect(
      createWidgetSubmission({
        config,
        formData: withImages(2),
        userAgent: null,
      })
    ).rejects.toThrow(`TX_FAILED`)
    const deletedKeys = vi
      .mocked(deleteObject)
      .mock.calls.map((call) => call[0])
    expect(new Set(deletedKeys).size).toBe(3)
  })
})

describe(`createWidgetSubmission notifications + solo auto-assign`, () => {
  beforeEach(() => {
    h.inserts.length = 0
    h.txShouldFail = false
    h.getSoleHumanMemberId.mockClear()
    h.getSoleHumanMemberId.mockResolvedValue(null)
    h.ensureSubscribed.mockClear()
    h.fireAndForgetNewIssueNotify.mockClear()
    h.fireAndForgetAssignmentNotify.mockClear()
  })

  it(`solo team: auto-assigns the sole member, subscribes them as assignee, fires only issue_created`, async () => {
    h.getSoleHumanMemberId.mockResolvedValue(`member-1`)

    const result = await createWidgetSubmission({
      config,
      formData: submitForm(),
      userAgent: null,
    })

    expect(h.getSoleHumanMemberId).toHaveBeenCalledWith(`ws-1`)
    expect(issueInsert()?.values.assigneeId).toBe(`member-1`)
    expect(issueInsert()?.values.creatorId).toBeNull()
    expect(issueInsert()?.values.source).toBe(`widget`)

    expect(h.ensureSubscribed).toHaveBeenCalledTimes(1)
    expect(h.ensureSubscribed).toHaveBeenCalledWith(tx, {
      issueId: result.issueId,
      userId: `member-1`,
      teamId: `ws-1`,
      source: `assignee`,
    })

    expect(h.fireAndForgetNewIssueNotify).toHaveBeenCalledTimes(1)
    expect(h.fireAndForgetNewIssueNotify).toHaveBeenCalledWith({
      issueId: result.issueId,
    })
    // The auto-assignment must NOT double-notify via an "assigned you" row.
    expect(h.fireAndForgetAssignmentNotify).not.toHaveBeenCalled()
  })

  it(`multi-member team: no assignee, no assignee subscription, members still notified`, async () => {
    const result = await createWidgetSubmission({
      config,
      formData: submitForm(),
      userAgent: null,
    })

    expect(issueInsert()?.values.assigneeId).toBeNull()
    expect(h.ensureSubscribed).not.toHaveBeenCalled()
    // No reporter email in the form → no subscriber row at all.
    expect(h.inserts.some((i) => i.table === issueSubscribers)).toBe(false)
    expect(h.fireAndForgetNewIssueNotify).toHaveBeenCalledWith({
      issueId: result.issueId,
    })
  })

  it(`does not notify when the transaction fails`, async () => {
    h.txShouldFail = true

    await expect(
      createWidgetSubmission({
        config,
        formData: submitForm(),
        userAgent: null,
      })
    ).rejects.toThrow(`TX_FAILED`)

    expect(h.fireAndForgetNewIssueNotify).not.toHaveBeenCalled()
  })

  // The panel's required-email gate is client-side only (and disappears when
  // the config fetch races the first open), so the server must enforce the
  // board owner's policy itself.
  describe(`emailRequired enforcement`, () => {
    const requiredConfig: WidgetConfigWithBoard = {
      ...config,
      formConfig: { emailRequired: true },
    }

    it(`rejects an email-less submission on a required-email board`, async () => {
      const attempt = createWidgetSubmission({
        config: requiredConfig,
        formData: submitForm(),
        userAgent: null,
      })

      await expect(attempt).rejects.toBeInstanceOf(WidgetRequestError)
      await expect(attempt).rejects.toMatchObject({
        status: 400,
        message: `Email is required`,
        code: `email_required`,
      })
      expect(h.inserts.length).toBe(0)
      expect(h.fireAndForgetNewIssueNotify).not.toHaveBeenCalled()
    })

    it(`accepts and records the reporter when the email is present`, async () => {
      const form = submitForm()
      form.set(`email`, `reporter@example.com`)

      await createWidgetSubmission({
        config: requiredConfig,
        formData: form,
        userAgent: null,
      })

      const subscriber = h.inserts.find((i) => i.table === issueSubscribers)
      expect(subscriber?.values.email).toBe(`reporter@example.com`)
      expect(subscriber?.values.source).toBe(`widget_reporter`)
    })

    it(`keeps email optional when the board does not require it`, async () => {
      await createWidgetSubmission({
        config,
        formData: submitForm(),
        userAgent: null,
      })

      expect(h.inserts.some((i) => i.table === issues)).toBe(true)
    })
  })
})

// SLOP-4: ONE submit path. The reporter's words arrive as `message` (a host
// may add a `title` via the loader's submit API) and are escaped ONCE into
// literal GFM; a reporter who left an email gets the
// confirmation carrying the magic conversation link, and the response says
// whether it went out.
describe(`createWidgetSubmission reporter text + confirmation (SLOP-4)`, () => {
  beforeEach(() => {
    h.inserts.length = 0
    h.dbInserts.length = 0
    h.txShouldFail = false
    h.getSoleHumanMemberId.mockClear()
    h.getSoleHumanMemberId.mockResolvedValue(null)
    h.ensureSubscribed.mockClear()
    h.fireAndForgetNewIssueNotify.mockClear()
    h.sendReporterConfirmationEmail.mockClear()
    h.emailEnabled.value = true
  })

  const messageForm = (message: string): FormData => {
    const form = new FormData()
    form.set(`message`, message)
    return form
  }

  it(`a message-only submit derives the title from the first line`, async () => {
    const result = await createWidgetSubmission({
      config,
      formData: messageForm(`My login is broken\nIt loops back to the form.`),
      userAgent: null,
    })

    expect(issueInsert()?.values.title).toBe(`My login is broken`)
    expect(issueInsert()?.values.description).toBe(
      `My login is broken\nIt loops back to the form.`
    )
    expect(issueInsert()?.values.source).toBe(`widget`)
    expect(result).toMatchObject({
      issueId: expect.any(String),
      identifier: `EXP-9`,
      url: null,
      emailDelivered: null,
    })
    expect(h.sendReporterConfirmationEmail).not.toHaveBeenCalled()
    expect(h.dbInserts.some((i) => i.table === emailDeliveries)).toBe(false)
  })

  it(`escapes the reporter's text so no markdown, mention or ref fires`, async () => {
    await createWidgetSubmission({
      config,
      formData: messageForm(`# not a heading\nping @bob@x.io re #EXP-1 <b>`),
      userAgent: null,
    })
    expect(issueInsert()?.values.title).toBe(`\\# not a heading`)
    expect(issueInsert()?.values.description).toBe(
      `\\# not a heading\nping \\@bob\\@x\\.io re \\#EXP-1 \\<b\\>`
    )
  })

  it(`emails the confirmation with the magic link when an email was given`, async () => {
    const form = messageForm(`Button broken`)
    form.set(`email`, `reporter@example.com`)
    form.set(`name`, `Ada`)

    const result = await createWidgetSubmission({
      config,
      formData: form,
      userAgent: null,
    })

    expect(result.emailDelivered).toBe(true)
    const issueId = issueInsert()?.values.id as string
    expect(h.sendReporterConfirmationEmail).toHaveBeenCalledWith({
      to: `reporter@example.com`,
      teamName: `Acme`,
      issueTitle: `Button broken`,
      conversationUrl: `https://app.test/support/${issueId}.mac`,
    })
    // The ledger row names the issue, never the URL.
    const ledger = h.dbInserts.find((i) => i.table === emailDeliveries)
    expect(ledger?.values).toMatchObject({
      kind: `reporter_confirmation`,
      toEmail: `reporter@example.com`,
      issueId,
      status: `sent`,
      userId: null,
    })
    expect(JSON.stringify(ledger?.values)).not.toContain(`.mac`)
    // The submission row carries the reporter identity the card + reply
    // toggle read.
    const submission = h.inserts.find((i) => i.table === widgetSubmissions)
    expect(submission?.values).toMatchObject({
      issueId,
      reporterEmail: `reporter@example.com`,
      reporterName: `Ada`,
    })
  })

  it(`reports emailDelivered false when the transport refuses the send`, async () => {
    h.sendReporterConfirmationEmail.mockResolvedValueOnce({
      delivered: false,
      provider: `ses`,
      messageId: null,
      subject: `x`,
    } as never)
    const form = messageForm(`Button broken`)
    form.set(`email`, `reporter@example.com`)

    const result = await createWidgetSubmission({
      config,
      formData: form,
      userAgent: null,
    })

    expect(result.emailDelivered).toBe(false)
    // The report itself is filed regardless.
    expect(h.inserts.some((i) => i.table === issues)).toBe(true)
    expect(h.fireAndForgetNewIssueNotify).toHaveBeenCalledTimes(1)
    const ledger = h.dbInserts.find((i) => i.table === emailDeliveries)
    expect(ledger?.values).toMatchObject({ status: `failed`, sentAt: null })
  })

  it(`reports emailDelivered false without a transport, recording nothing`, async () => {
    h.emailEnabled.value = false
    const form = messageForm(`Button broken`)
    form.set(`email`, `reporter@example.com`)

    const result = await createWidgetSubmission({
      config,
      formData: form,
      userAgent: null,
    })

    expect(result.emailDelivered).toBe(false)
    expect(h.sendReporterConfirmationEmail).not.toHaveBeenCalled()
    expect(h.dbInserts.some((i) => i.table === emailDeliveries)).toBe(false)
  })

  it(`a failed confirmation send never fails the committed report`, async () => {
    h.sendReporterConfirmationEmail.mockRejectedValueOnce(new Error(`boom`))
    const form = messageForm(`Button broken`)
    form.set(`email`, `reporter@example.com`)

    const result = await createWidgetSubmission({
      config,
      formData: form,
      userAgent: null,
    })

    expect(result.emailDelivered).toBe(false)
    expect(h.inserts.some((i) => i.table === issues)).toBe(true)
  })

  it(`a host-passed title files escaped, the message as the description`, async () => {
    const form = new FormData()
    form.set(`title`, `Button *broken*`)
    form.set(`message`, `Clicking does nothing.`)

    await createWidgetSubmission({ config, formData: form, userAgent: null })

    expect(issueInsert()?.values.title).toBe(`Button \\*broken\\*`)
    expect(issueInsert()?.values.description).toBe(`Clicking does nothing.`)
  })

  it(`caps a host-passed title AFTER escaping so it fits issues.title`, async () => {
    const form = new FormData()
    // 500 chars that escape to 1000.
    form.set(`title`, `*`.repeat(500))
    form.set(`message`, `Clicking does nothing.`)

    await createWidgetSubmission({ config, formData: form, userAgent: null })

    const title = issueInsert()?.values.title as string
    expect(title.length).toBeLessThanOrEqual(500)
    expect(title.endsWith(`…`)).toBe(true)
    // Whole escapes only: no lone backslash before the ellipsis.
    expect(title).toMatch(/^(\\\*)+…$/)
  })

  it(`ignores the retired description field (pre-SLOP-4 bundles expired)`, async () => {
    const form = new FormData()
    form.set(`description`, `the description`)
    form.set(`screenshot`, new Blob([`x`], { type: `image/png` }), `s.png`)

    await expect(
      createWidgetSubmission({ config, formData: form, userAgent: null })
    ).rejects.toMatchObject({ status: 400 })
  })

  it(`uses the title fallback for a text-less report with only a screenshot`, async () => {
    const form = new FormData()
    form.set(`message`, `   `)
    form.set(
      `screenshot`,
      new File([new Uint8Array([1])], `shot.png`, { type: `image/png` })
    )
    await expect(
      createWidgetSubmission({ config, formData: form, userAgent: null })
    ).rejects.toMatchObject({ status: 400 })
    expect(h.inserts.length).toBe(0)
  })

  it(`a stale cached bundle's mode field is ignored`, async () => {
    const form = messageForm(`Please help me`)
    form.set(`mode`, `support`)
    form.set(`email`, `reporter@example.com`)

    const result = await createWidgetSubmission({
      config,
      formData: form,
      userAgent: null,
    })

    expect(result.issueId).toEqual(expect.any(String))
    expect(h.inserts.some((i) => i.table === issues)).toBe(true)
  })

  it(`refuses writes while the board is trashed or archived`, async () => {
    for (const patch of [
      { boardDeletedAt: new Date() },
      { boardArchivedAt: new Date() },
    ]) {
      await expect(
        createWidgetSubmission({
          config: { ...config, ...patch } as WidgetConfigWithBoard,
          formData: messageForm(`x`),
          userAgent: null,
        })
      ).rejects.toMatchObject({ status: 403 })
    }
    expect(h.inserts.length).toBe(0)
  })
})

// REV-25: the anonymous GET /api/widget/config takes a per-IP token BEFORE
// any DB work. The db mock in this file has no innerJoin chain, so a
// well-formed key that reached loadWidgetConfigByKey would surface as a 500
// — the 429/404 statuses below double as proof the lookup never ran.
describe(`handleWidgetConfig rate limiting`, () => {
  beforeEach(() => {
    h.configIpTryTake.mockClear()
    h.configIpTryTake.mockReturnValue({ ok: true })
  })

  const configRequest = (key: string, origin?: string) =>
    new Request(`https://app.test/api/widget/config?key=${key}`, {
      headers: origin ? { origin } : {},
    })

  it(`429s with Retry-After before the key lookup when the bucket is empty`, async () => {
    h.configIpTryTake.mockReturnValue({ ok: false, retryAfterSeconds: 42 })
    const res = await handleWidgetConfig(
      configRequest(`expw_${`a`.repeat(32)}`, `https://example.com`)
    )
    expect(res.status).toBe(429)
    expect(res.headers.get(`Retry-After`)).toBe(`42`)
    // Permissive origin echo, like the preflight handler — the body carries
    // no data and the real config response stays allowlist-gated.
    expect(res.headers.get(`Access-Control-Allow-Origin`)).toBe(
      `https://example.com`
    )
    expect(h.configIpTryTake).toHaveBeenCalledWith(`ip:203.0.113.7`)
  })

  it(`spends a token even on malformed keys`, async () => {
    const res = await handleWidgetConfig(configRequest(`nope`))
    expect(res.status).toBe(404)
    expect(h.configIpTryTake).toHaveBeenCalledTimes(1)
  })
})

// The submit route relays WidgetRequestError.code into the JSON body; the
// client uses it to re-reveal a hidden identity-email input. Validation
// behavior (statuses/messages) is unchanged — only the code is additive.
describe(`structured email error codes`, () => {
  beforeEach(() => {
    h.inserts.length = 0
  })

  it(`flags invalid_email when a feedback email is malformed`, async () => {
    const form = submitForm()
    form.set(`email`, `user#tag@example.com`)
    await expect(
      createWidgetSubmission({ config, formData: form, userAgent: null })
    ).rejects.toMatchObject({ status: 400, code: `invalid_email` })
    expect(h.inserts.length).toBe(0)
  })

  it(`flags invalid_email on a message-only submit too`, async () => {
    const form = new FormData()
    form.set(`message`, `Please help me`)
    form.set(`email`, `user#tag@example.com`)
    await expect(
      createWidgetSubmission({ config, formData: form, userAgent: null })
    ).rejects.toMatchObject({ status: 400, code: `invalid_email` })
    expect(h.inserts.length).toBe(0)
  })

  it(`leaves a non-email field failure uncoded`, async () => {
    const form = submitForm()
    form.set(`name`, `x`.repeat(300))
    const error = await createWidgetSubmission({
      config,
      formData: form,
      userAgent: null,
    }).catch((caught: unknown) => caught)
    expect(error).toBeInstanceOf(WidgetRequestError)
    expect((error as WidgetRequestError).status).toBe(400)
    expect((error as WidgetRequestError).code).toBeUndefined()
  })
})

// EXP-244: the panel's name gate is advisory like the email one — the config
// may be up to 5 minutes stale and cached pre-name bundles render no field at
// all — so the owner's policy is enforced server-side on submit.
describe(`nameRequired enforcement`, () => {
  const nameRequiredConfig = {
    ...config,
    formConfig: { collectName: true, nameRequired: true },
  } as unknown as WidgetConfigWithBoard

  beforeEach(() => {
    h.inserts.length = 0
    h.fireAndForgetNewIssueNotify.mockClear()
  })

  it(`rejects a name-less submission with name_required`, async () => {
    await expect(
      createWidgetSubmission({
        config: nameRequiredConfig,
        formData: submitForm(),
        userAgent: null,
      })
    ).rejects.toMatchObject({
      status: 400,
      message: `Name is required`,
      code: `name_required`,
    })
    expect(h.inserts.length).toBe(0)
  })

  it(`accepts and records the reporter name when present`, async () => {
    const form = submitForm()
    form.set(`name`, `dani`)
    await createWidgetSubmission({
      config: nameRequiredConfig,
      formData: form,
      userAgent: null,
    })
    const submission = h.inserts.find((i) => i.table === widgetSubmissions)
    expect(submission?.values.reporterName).toBe(`dani`)
  })

  it(`keeps name optional when not required`, async () => {
    await createWidgetSubmission({
      config: {
        ...config,
        formConfig: { collectName: true },
      } as unknown as WidgetConfigWithBoard,
      formData: submitForm(),
      userAgent: null,
    })
    expect(h.inserts.some((i) => i.table === issues)).toBe(true)
  })
})

// The submit path must enforce the SAME normalized toggle view the config
// route serves — a raw `{nameRequired: true}` row without collectName
// (writable via a direct tRPC call; the settings UI can't produce it) used to
// serve a form with no name field while 400ing every submit: a permanently
// broken widget.
describe(`served/enforced toggle agreement`, () => {
  beforeEach(() => {
    h.inserts.length = 0
    h.fireAndForgetNewIssueNotify.mockClear()
  })

  it(`ignores nameRequired without collectName`, async () => {
    await createWidgetSubmission({
      config: {
        ...config,
        formConfig: { nameRequired: true },
      } as unknown as WidgetConfigWithBoard,
      formData: submitForm(),
      userAgent: null,
    })
    expect(h.inserts.some((i) => i.table === issues)).toBe(true)
  })

  it(`emailRequired still enforces when collectEmail is written false`, async () => {
    // Required implies collect: the served form always shows the email field
    // for this bag, so enforcing the requirement stays consistent.
    await expect(
      createWidgetSubmission({
        config: {
          ...config,
          formConfig: { emailRequired: true, collectEmail: false },
        } as unknown as WidgetConfigWithBoard,
        formData: submitForm(),
        userAgent: null,
      })
    ).rejects.toMatchObject({ status: 400, code: `email_required` })
    expect(h.inserts.length).toBe(0)
  })
})

// The ONE normalizer both the config route and the submit path read —
// locked here so the rules can't drift apart again.
describe(`normalizedWidgetFormToggles`, () => {
  it(`defaults: email shown optional, name hidden`, () => {
    expect(normalizedWidgetFormToggles(null)).toEqual({
      emailRequired: false,
      collectEmail: true,
      collectName: false,
      nameRequired: false,
    })
  })

  it(`required implies collect for email`, () => {
    expect(
      normalizedWidgetFormToggles({ emailRequired: true, collectEmail: false })
    ).toEqual({
      emailRequired: true,
      collectEmail: true,
      collectName: false,
      nameRequired: false,
    })
  })

  it(`a hidden name field is never required`, () => {
    expect(normalizedWidgetFormToggles({ nameRequired: true })).toMatchObject({
      collectName: false,
      nameRequired: false,
    })
    expect(
      normalizedWidgetFormToggles({ collectName: true, nameRequired: true })
    ).toMatchObject({ collectName: true, nameRequired: true })
  })
})

// EXP-244: custom field values ride the merged customData blob, so
// requiredness is checked against it — a host setCustomData value satisfies
// a required field just like a panel-typed one.
describe(`required custom fields enforcement`, () => {
  const fieldsConfig = {
    ...config,
    formConfig: {
      customFields: [
        { key: `desk`, label: `Desk number`, required: true },
        { key: `mood`, label: `Mood` },
      ],
    },
  } as unknown as WidgetConfigWithBoard

  beforeEach(() => {
    h.inserts.length = 0
    h.fireAndForgetNewIssueNotify.mockClear()
  })

  it(`rejects when a required field is missing from customData`, async () => {
    await expect(
      createWidgetSubmission({
        config: fieldsConfig,
        formData: submitForm(),
        userAgent: null,
      })
    ).rejects.toMatchObject({
      status: 400,
      message: `Please fill in "Desk number"`,
    })
    expect(h.inserts.length).toBe(0)
  })

  it(`rejects a whitespace-only required value`, async () => {
    const form = submitForm()
    form.set(`customData`, JSON.stringify({ desk: `   ` }))
    await expect(
      createWidgetSubmission({
        config: fieldsConfig,
        formData: form,
        userAgent: null,
      })
    ).rejects.toMatchObject({ status: 400 })
  })

  it(`accepts when the required value is present (optional field may stay empty)`, async () => {
    const form = submitForm()
    form.set(`customData`, JSON.stringify({ desk: `42b` }))
    await createWidgetSubmission({
      config: fieldsConfig,
      formData: form,
      userAgent: null,
    })
    const submission = h.inserts.find((i) => i.table === widgetSubmissions)
    expect(submission?.values.customData).toEqual({ desk: `42b` })
  })

  it(`treats numbers and booleans as present`, async () => {
    const form = submitForm()
    form.set(`customData`, JSON.stringify({ desk: 42 }))
    await createWidgetSubmission({
      config: fieldsConfig,
      formData: form,
      userAgent: null,
    })
    expect(h.inserts.some((i) => i.table === issues)).toBe(true)
  })

  // The key pattern admits prototype names ("Constructor" slugifies to
  // `constructor`) — the presence check must read OWN properties only, never
  // the inherited Object constructor.
  it(`reads own properties only for prototype-named keys`, async () => {
    const protoConfig = {
      ...config,
      formConfig: {
        customFields: [
          { key: `constructor`, label: `Constructor`, required: true },
        ],
      },
    } as unknown as WidgetConfigWithBoard

    const empty = submitForm()
    empty.set(`customData`, JSON.stringify({}))
    await expect(
      createWidgetSubmission({
        config: protoConfig,
        formData: empty,
        userAgent: null,
      })
    ).rejects.toMatchObject({
      status: 400,
      message: `Please fill in "Constructor"`,
    })

    const form = submitForm()
    form.set(`customData`, JSON.stringify({ constructor: `MyWidget` }))
    await createWidgetSubmission({
      config: protoConfig,
      formData: form,
      userAgent: null,
    })
    expect(h.inserts.some((i) => i.table === issues)).toBe(true)
  })
})

// form_config is untyped jsonb — the read-side sanitizer is what stands
// between a hand-edited row and third-party pages.
describe(`sanitizeWidgetCustomFields`, () => {
  it(`returns [] for absent/junk configs`, () => {
    expect(sanitizeWidgetCustomFields(null)).toEqual([])
    expect(sanitizeWidgetCustomFields({})).toEqual([])
    expect(sanitizeWidgetCustomFields({ customFields: `nope` })).toEqual([])
    expect(sanitizeWidgetCustomFields({ customFields: [1, null, `x`] })).toEqual(
      []
    )
  })

  it(`drops malformed entries, dedupes keys, normalizes required`, () => {
    expect(
      sanitizeWidgetCustomFields({
        customFields: [
          { key: `desk`, label: `Desk`, required: `yes` },
          { key: `desk`, label: `Duplicate` },
          { key: `BAD KEY`, label: `Bad` },
          { key: `mood`, label: `  Mood  `, required: true },
          { key: `empty`, label: `   ` },
        ],
      })
    ).toEqual([
      { key: `desk`, label: `Desk`, required: false },
      { key: `mood`, label: `Mood`, required: true },
    ])
  })

  it(`caps at 8 fields and truncates labels to 40 chars`, () => {
    const fields = Array.from({ length: 10 }, (_, i) => ({
      key: `f${i}`,
      label: `L`.repeat(60),
    }))
    const out = sanitizeWidgetCustomFields({ customFields: fields })
    expect(out.length).toBe(8)
    expect(out[0].label.length).toBe(40)
  })
})

// EXP-435: reporter-picked labels — defensive form_config reads, config-time
// resolution against live rows, and the in-tx issue_labels insert.
const lid = (n: number) =>
  `00000000-0000-4000-8000-${String(n).padStart(12, `0`)}`

describe(`sanitizeWidgetLabelIds / theme / hex color`, () => {
  it(`returns [] for absent/junk configs`, () => {
    expect(sanitizeWidgetLabelIds(null)).toEqual([])
    expect(sanitizeWidgetLabelIds({})).toEqual([])
    expect(sanitizeWidgetLabelIds({ labelIds: `nope` })).toEqual([])
  })

  it(`drops non-strings and non-UUIDs, dedupes, caps at 10`, () => {
    const ids = Array.from({ length: 12 }, (_, i) => lid(i))
    expect(
      sanitizeWidgetLabelIds({
        labelIds: [lid(1), 7, ``, `not-a-uuid`, lid(1), lid(2)],
      })
    ).toEqual([lid(1), lid(2)])
    expect(sanitizeWidgetLabelIds({ labelIds: ids })).toHaveLength(10)
  })

  it(`sanitizeWidgetTheme accepts only the three values`, () => {
    expect(sanitizeWidgetTheme({ theme: `light` })).toBe(`light`)
    expect(sanitizeWidgetTheme({ theme: `auto` })).toBe(`auto`)
    expect(sanitizeWidgetTheme({ theme: `dark` })).toBe(`dark`)
    expect(sanitizeWidgetTheme({ theme: `LIGHT` })).toBeNull()
    expect(sanitizeWidgetTheme(null)).toBeNull()
  })

  it(`sanitizeWidgetHexColor accepts only #hex6`, () => {
    expect(sanitizeWidgetHexColor(`#0a0A0a`)).toBe(`#0a0A0a`)
    expect(sanitizeWidgetHexColor(`#fff`)).toBeNull()
    expect(sanitizeWidgetHexColor(`red`)).toBeNull()
    expect(sanitizeWidgetHexColor(7)).toBeNull()
  })

  // EXP-569 launcher resolution: stored launcher > the defaults; junk
  // degrades per device. (EXP-672 dropped the legacy `position` read tier.)
  it(`sanitizeWidgetLauncher defaults bare rows to the new placements`, () => {
    expect(sanitizeWidgetLauncher(null)).toEqual({
      desktop: { mode: `fab`, position: `bottom-right` },
      mobile: { mode: `tab`, position: `middle-right` },
      icon: null,
    })
    expect(sanitizeWidgetLauncher({})).toEqual({
      ...defaultWidgetLauncher,
      icon: null,
    })
    // The widget package ships the same defaults — a mismatch makes the
    // launcher jump when the config fetch resolves over the pre-config
    // render.
    expect(widgetPackageDefaultLauncher).toEqual(defaultWidgetLauncher)
  })

  // EXP-672: every stored row carries `launcher`, so a leftover legacy
  // `position` is ignored outright.
  it(`sanitizeWidgetLauncher ignores a stored legacy position`, () => {
    expect(sanitizeWidgetLauncher({ position: `bottom-left` })).toEqual({
      ...defaultWidgetLauncher,
      icon: null,
    })
  })

  it(`sanitizeWidgetLauncher prefers stored launcher entries, per device`, () => {
    expect(
      sanitizeWidgetLauncher({
        launcher: {
          desktop: { mode: `tab`, position: `middle-left` },
          mobile: { mode: `pill`, position: `middle-left` },
          icon: `bug`,
        },
      })
    ).toEqual({
      desktop: { mode: `tab`, position: `middle-left` },
      // The invalid mobile entry degrades to the default.
      mobile: defaultWidgetLauncher.mobile,
      icon: `bug`,
    })
  })

  it(`sanitizeWidgetLauncher validates the icon against the pickable set`, () => {
    const launcherWithIcon = (icon: unknown) =>
      sanitizeWidgetLauncher({ launcher: { icon } }).icon
    expect(launcherWithIcon(`megaphone`)).toBe(`megaphone`)
    expect(launcherWithIcon(`not-an-icon`)).toBeNull()
    expect(launcherWithIcon(7)).toBeNull()
    expect(sanitizeWidgetLauncher({ launcher: `junk` }).icon).toBeNull()
  })
})

describe(`resolveWidgetConfigLabels`, () => {
  beforeEach(() => {
    h.labelSelectRows.length = 0
  })

  it(`resolves configured ids to live rows`, async () => {
    h.labelSelectRows.push({ id: lid(1), name: `Bug`, color: `#ef4444` })
    const out = await resolveWidgetConfigLabels({
      ...config,
      formConfig: { labelIds: [lid(1), lid(9)] },
    } as unknown as WidgetConfigWithBoard)
    expect(out).toEqual([{ id: lid(1), name: `Bug`, color: `#ef4444` }])
  })

  it(`skips the query entirely when no labels are configured`, async () => {
    const out = await resolveWidgetConfigLabels(config)
    expect(out).toEqual([])
  })
})

describe(`widget label selection on submit`, () => {
  const labelConfig = {
    ...config,
    formConfig: { labelIds: [lid(1), lid(2)] },
  } as unknown as WidgetConfigWithBoard

  beforeEach(() => {
    h.inserts.length = 0
    h.txShouldFail = false
    h.labelSelectRows.length = 0
    h.getSoleHumanMemberId.mockResolvedValue(null)
  })

  const withLabels = (labels: unknown) => {
    const form = submitForm()
    form.set(`labels`, JSON.stringify(labels))
    return form
  }

  it(`inserts issue_labels rows carrying all four columns`, async () => {
    h.labelSelectRows.push({ id: lid(1) }, { id: lid(2) })
    await createWidgetSubmission({
      config: labelConfig,
      formData: withLabels([lid(1), lid(2)]),
      userAgent: null,
    })
    const insert = h.inserts.find((i) => i.table === issueLabels)
    expect(insert).toBeDefined()
    const rows = insert!.values as unknown as Array<Record<string, unknown>>
    expect(rows).toHaveLength(2)
    expect(rows[0]).toMatchObject({
      labelId: lid(1),
      teamId: `ws-1`,
      boardId: `proj-1`,
    })
    expect(typeof rows[0].issueId).toBe(`string`)
  })

  it(`silently drops ids outside the configured set (stale cached config)`, async () => {
    h.labelSelectRows.push({ id: lid(1) })
    await createWidgetSubmission({
      config: labelConfig,
      formData: withLabels([lid(1), lid(7)]),
      userAgent: null,
    })
    const insert = h.inserts.find((i) => i.table === issueLabels)
    const rows = insert!.values as unknown as Array<Record<string, unknown>>
    expect(rows.map((row) => row.labelId)).toEqual([lid(1)])
  })

  it(`drops labels deleted since the config write (empty re-select)`, async () => {
    // Configured and submitted, but the in-tx select finds nothing.
    await createWidgetSubmission({
      config: labelConfig,
      formData: withLabels([lid(1)]),
      userAgent: null,
    })
    expect(h.inserts.find((i) => i.table === issueLabels)).toBeUndefined()
  })

  it(`no labels field → no issue_labels insert`, async () => {
    await createWidgetSubmission({
      config: labelConfig,
      formData: submitForm(),
      userAgent: null,
    })
    expect(h.inserts.find((i) => i.table === issueLabels)).toBeUndefined()
  })

  it(`rejects malformed labels payloads`, async () => {
    const form = submitForm()
    form.set(`labels`, `not-json`)
    await expect(
      createWidgetSubmission({
        config: labelConfig,
        formData: form,
        userAgent: null,
      })
    ).rejects.toMatchObject({ status: 400, message: `Invalid labels` })

    await expect(
      createWidgetSubmission({
        config: labelConfig,
        formData: withLabels([`l-1`, 7]),
        userAgent: null,
      })
    ).rejects.toMatchObject({ status: 400, message: `Invalid labels` })
  })
})
