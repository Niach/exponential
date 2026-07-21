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
  fireAndForgetSupportThreadNotify: vi.fn(),
  assertCanUseHelpdesk: vi.fn(async (): Promise<void> => undefined),
  createSupportThreadInTx: vi.fn(
    async (_tx: unknown, _args: Record<string, unknown>) => ({
      threadId: `thread-1`,
      token: `tok-minted`,
    })
  ),
  sendSupportConfirmationEmail: vi.fn(async () => ({
    delivered: true,
    provider: `ses`,
    messageId: `msg-1`,
  })),
  inserts: [] as Array<{ table: unknown; values: Record<string, unknown> }>,
  // Post-commit inserts (the email-delivery ledger) go through db.insert.
  dbInserts: [] as Array<{ table: unknown; values: Record<string, unknown> }>,
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
      }
    },
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
  },
}))

// lib/trpc.ts imports `auth`/db at module scope; only generateTxId is used.
vi.mock(`@/lib/trpc`, () => ({ generateTxId: vi.fn(async () => 1) }))
vi.mock(`@/lib/billing`, () => ({
  assertWithinStorageLimit: vi.fn(async () => undefined),
  assertCanUseHelpdesk: h.assertCanUseHelpdesk,
}))
vi.mock(`@/lib/helpdesk/service`, () => ({
  createSupportThreadInTx: h.createSupportThreadInTx,
  MAX_SUPPORT_MESSAGE_CHARS: 10_000,
  supportThreadUrl: (token: string) => `https://app.test/support/${token}`,
  // Mirrors the real first-line clamp — the support path titles threads with it.
  supportTicketTitle: (message: string) => {
    const firstLine = (message.split(`\n`, 1)[0] ?? ``).trim()
    if (!firstLine) return `Support request`
    return firstLine.length > 120
      ? `${firstLine.slice(0, 119).trimEnd()}…`
      : firstLine
  },
}))
vi.mock(`@/lib/email`, () => ({
  sendSupportConfirmationEmail: h.sendSupportConfirmationEmail,
}))
vi.mock(`@/lib/storage/issue-attachments`, () => ({
  buildAttachmentStorageKey: vi.fn(() => `key`),
  buildAttachmentUrl: vi.fn(() => `/api/attachments/x`),
  maxImageUploadBytes: 10 * 1024 * 1024,
  sanitizeUploadFilename: vi.fn(() => `screenshot.png`),
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
  fireAndForgetSupportThreadNotify: h.fireAndForgetSupportThreadNotify,
}))

import {
  emailDeliveries,
  issues,
  issueSubscribers,
  widgetSubmissions,
} from "@/db/schema"
import {
  createWidgetSubmission,
  createWidgetSupportSubmission,
  effectiveWidgetModes,
  requestedWidgetModes,
  WidgetRequestError,
  type WidgetConfigWithBoard,
} from "@/lib/widget/service"

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
  boardArchivedAt: null,
  teamSlug: `acme`,
  teamHelpdeskEnabled: false,
} as unknown as WidgetConfigWithBoard

// A config whose support mode is fully live (team helpdesk on, plan gate
// mocked green).
const supportConfig = {
  ...config,
  formConfig: { modes: [`feedback`, `support`] },
  teamHelpdeskEnabled: true,
} as unknown as WidgetConfigWithBoard

function submitForm(): FormData {
  const form = new FormData()
  form.set(`title`, `Button broken`)
  return form
}

const issueInsert = () => h.inserts.find((i) => i.table === issues)

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

// The widget's support mode files a STANDALONE helpdesk ticket (EXP-180):
// a support thread + widget_submissions context row in one transaction — no
// issue — then the confirmation email carrying the magic link (emails are the
// token's only carrier — it is never stored).
describe(`createWidgetSupportSubmission`, () => {
  beforeEach(() => {
    h.inserts.length = 0
    h.dbInserts.length = 0
    h.txShouldFail = false
    h.getSoleHumanMemberId.mockClear()
    h.getSoleHumanMemberId.mockResolvedValue(null)
    h.ensureSubscribed.mockClear()
    h.fireAndForgetNewIssueNotify.mockClear()
    h.fireAndForgetSupportThreadNotify.mockClear()
    h.assertCanUseHelpdesk.mockClear()
    h.assertCanUseHelpdesk.mockResolvedValue(undefined)
    h.createSupportThreadInTx.mockClear()
    h.sendSupportConfirmationEmail.mockClear()
  })

  const supportForm = (): FormData => {
    const form = new FormData()
    form.set(`mode`, `support`)
    form.set(`message`, `My login is broken\nIt loops back to the form.`)
    form.set(`email`, `reporter@example.com`)
    return form
  }

  it(`files a standalone ticket: thread + context row + confirmation email, NO issue`, async () => {
    const result = await createWidgetSupportSubmission({
      config: supportConfig,
      formData: supportForm(),
      userAgent: `UA`,
    })

    // No issue, no subscriber row — the ticket is thread-only.
    expect(h.inserts.some((i) => i.table === issues)).toBe(false)
    expect(h.inserts.some((i) => i.table === issueSubscribers)).toBe(false)

    expect(h.createSupportThreadInTx).toHaveBeenCalledTimes(1)
    expect(h.createSupportThreadInTx.mock.calls[0][1]).toMatchObject({
      teamId: `ws-1`,
      title: `My login is broken`,
      reporterEmail: `reporter@example.com`,
    })

    const submission = h.inserts.find((i) => i.table === widgetSubmissions)
    expect(submission?.values.supportThreadId).toBe(`thread-1`)
    expect(submission?.values.issueId).toBeNull()

    // Members are notified through the support fan-out, not issue_created.
    expect(h.fireAndForgetSupportThreadNotify).toHaveBeenCalledWith({
      threadId: `thread-1`,
      kind: `created`,
    })
    expect(h.fireAndForgetNewIssueNotify).not.toHaveBeenCalled()

    expect(h.sendSupportConfirmationEmail).toHaveBeenCalledWith(
      expect.objectContaining({
        to: `reporter@example.com`,
        boardName: `Board`,
        threadUrl: `https://app.test/support/tok-minted`,
      })
    )
    const delivery = h.dbInserts.find((i) => i.table === emailDeliveries)
    expect(delivery?.values.kind).toBe(`support_confirmation`)
    expect(delivery?.values.status).toBe(`sent`)
    expect(delivery?.values.issueId).toBeNull()

    // Support tickets never mint an issue identifier or URL.
    expect(result.issueId).toBeNull()
    expect(result.identifier).toBeNull()
    expect(result.url).toBeNull()
  })

  it(`rejects when support mode is not enabled on the config`, async () => {
    await expect(
      createWidgetSupportSubmission({
        config,
        formData: supportForm(),
        userAgent: null,
      })
    ).rejects.toMatchObject({ status: 403 })
    expect(h.inserts).toHaveLength(0)
  })

  it(`rejects when the team helpdesk is off`, async () => {
    const stale = {
      ...supportConfig,
      teamHelpdeskEnabled: false,
    } as unknown as WidgetConfigWithBoard
    await expect(
      createWidgetSupportSubmission({
        config: stale,
        formData: supportForm(),
        userAgent: null,
      })
    ).rejects.toMatchObject({ status: 403 })
  })

  it(`rejects when the plan gate refuses the helpdesk`, async () => {
    h.assertCanUseHelpdesk.mockRejectedValue(new Error(`plan`))
    await expect(
      createWidgetSupportSubmission({
        config: supportConfig,
        formData: supportForm(),
        userAgent: null,
      })
    ).rejects.toMatchObject({ status: 403 })
  })

  it(`requires the reporter email`, async () => {
    const form = supportForm()
    form.delete(`email`)
    await expect(
      createWidgetSupportSubmission({
        config: supportConfig,
        formData: form,
        userAgent: null,
      })
    ).rejects.toMatchObject({ status: 400 })
  })

  it(`a failed confirmation email never fails the committed ticket`, async () => {
    h.sendSupportConfirmationEmail.mockRejectedValue(new Error(`SES down`))
    const result = await createWidgetSupportSubmission({
      config: supportConfig,
      formData: supportForm(),
      userAgent: null,
    })
    expect(result.identifier).toBeNull()
    expect(h.fireAndForgetSupportThreadNotify).toHaveBeenCalledTimes(1)
  })

  it(`a support ticket still files while the FEEDBACK board is trashed`, async () => {
    const feedbackTrashed = {
      ...supportConfig,
      boardDeletedAt: new Date(),
    } as unknown as WidgetConfigWithBoard

    await createWidgetSupportSubmission({
      config: feedbackTrashed,
      formData: supportForm(),
      userAgent: null,
    })
    expect(h.createSupportThreadInTx).toHaveBeenCalledTimes(1)

    // …while the feedback path still refuses the trashed board.
    await expect(
      createWidgetSubmission({
        config: feedbackTrashed,
        formData: submitForm(),
        userAgent: null,
      })
    ).rejects.toMatchObject({ status: 403 })
  })

  it(`the feedback path refuses a support-only widget`, async () => {
    const supportOnly = {
      ...supportConfig,
      formConfig: { modes: [`support`] },
    } as unknown as WidgetConfigWithBoard
    await expect(
      createWidgetSubmission({
        config: supportOnly,
        formData: submitForm(),
        userAgent: null,
      })
    ).rejects.toMatchObject({ status: 403 })
  })

  it(`a board-less (support-only) widget refuses feedback POSTs`, async () => {
    const boardless = {
      ...supportConfig,
      boardId: null,
      boardName: null,
      boardSlug: null,
      formConfig: { modes: [`support`] },
    } as unknown as WidgetConfigWithBoard
    await expect(
      createWidgetSubmission({
        config: boardless,
        formData: submitForm(),
        userAgent: null,
      })
    ).rejects.toMatchObject({ status: 403 })

    // Its support path works and the confirmation email falls back to the
    // widget's own name.
    await createWidgetSupportSubmission({
      config: boardless,
      formData: supportForm(),
      userAgent: null,
    })
    expect(h.sendSupportConfirmationEmail).toHaveBeenCalledWith(
      expect.objectContaining({ boardName: boardless.name })
    )
  })
})

describe(`widget modes`, () => {
  beforeEach(() => {
    h.assertCanUseHelpdesk.mockClear()
    h.assertCanUseHelpdesk.mockResolvedValue(undefined)
  })

  it(`defaults to feedback-only for pre-modes configs`, () => {
    expect(requestedWidgetModes(config)).toEqual([`feedback`])
  })

  it(`ignores junk values and dedupes`, () => {
    const junk = {
      ...config,
      formConfig: { modes: [`support`, `support`, `roadmap`] },
    } as unknown as WidgetConfigWithBoard
    expect(requestedWidgetModes(junk)).toEqual([`support`])
  })

  it(`drops support when the team helpdesk is off, keeping feedback`, async () => {
    const stale = {
      ...supportConfig,
      teamHelpdeskEnabled: false,
    } as unknown as WidgetConfigWithBoard
    expect(await effectiveWidgetModes(stale)).toEqual([`feedback`])
  })

  it(`a support-only widget with support unavailable serves nothing`, async () => {
    h.assertCanUseHelpdesk.mockRejectedValue(new Error(`plan`))
    const supportOnly = {
      ...supportConfig,
      formConfig: { modes: [`support`] },
    } as unknown as WidgetConfigWithBoard
    expect(await effectiveWidgetModes(supportOnly)).toEqual([])
  })

  it(`a board-less widget never offers feedback`, async () => {
    const boardless = {
      ...supportConfig,
      boardId: null,
    } as unknown as WidgetConfigWithBoard
    expect(await effectiveWidgetModes(boardless)).toEqual([`support`])
  })

  it(`serves both modes when everything is live`, async () => {
    expect(await effectiveWidgetModes(supportConfig)).toEqual([
      `feedback`,
      `support`,
    ])
  })
})

// The submit route relays WidgetRequestError.code into the JSON body; the
// client uses it to re-reveal a hidden identity-email input. Validation
// behavior (statuses/messages) is unchanged — only the code is additive.
describe(`structured email error codes`, () => {
  beforeEach(() => {
    h.inserts.length = 0
    h.assertCanUseHelpdesk.mockClear()
    h.assertCanUseHelpdesk.mockResolvedValue(undefined)
  })

  it(`flags invalid_email when a feedback email is malformed`, async () => {
    const form = submitForm()
    form.set(`email`, `user#tag@example.com`)
    await expect(
      createWidgetSubmission({ config, formData: form, userAgent: null })
    ).rejects.toMatchObject({ status: 400, code: `invalid_email` })
    expect(h.inserts.length).toBe(0)
  })

  it(`flags invalid_email when a support email is malformed`, async () => {
    const form = new FormData()
    form.set(`mode`, `support`)
    form.set(`message`, `Please help me`)
    form.set(`email`, `user#tag@example.com`)
    await expect(
      createWidgetSupportSubmission({
        config: supportConfig,
        formData: form,
        userAgent: null,
      })
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
