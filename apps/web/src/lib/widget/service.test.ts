import { beforeEach, describe, expect, it, vi } from "vitest"

// Locks EXP-53 + EXP-50 on the widget submit path: after the transaction
// commits, every human workspace member is notified via
// fireAndForgetNewIssueNotify (issue_created); in a solo workspace the issue
// is inserted with the sole human member as assignee (subscribed as
// `assignee`, NO assignment notification — issue_created already covers it).

const h = vi.hoisted(() => ({
  getSoleHumanMemberId: vi.fn(async (): Promise<string | null> => null),
  ensureSubscribed: vi.fn(),
  fireAndForgetNewIssueNotify: vi.fn(),
  fireAndForgetAssignmentNotify: vi.fn(),
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
vi.mock(`@/lib/workspace-membership`, () => ({
  getSoleHumanMemberId: h.getSoleHumanMemberId,
}))
vi.mock(`@/lib/integrations/subscriptions`, () => ({
  ensureSubscribed: h.ensureSubscribed,
}))
vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetNewIssueNotify: h.fireAndForgetNewIssueNotify,
  fireAndForgetAssignmentNotify: h.fireAndForgetAssignmentNotify,
}))

import { emailDeliveries, issues, issueSubscribers } from "@/db/schema"
import {
  createWidgetSubmission,
  createWidgetSupportSubmission,
  effectiveWidgetModes,
  requestedWidgetModes,
  supportTicketTitle,
  WidgetRequestError,
  type WidgetConfigWithProject,
} from "@/lib/widget/service"

const config = {
  id: `cfg-1`,
  workspaceId: `ws-1`,
  projectId: `proj-1`,
  widgetUserId: `widget-bot`,
  publicKey: `expw_test`,
  enabled: true,
  allowedDomains: [],
  formConfig: null,
  projectSlug: `board`,
  projectName: `Board`,
  // A private board keeps publicIssueUrl() out of play (no appBaseUrl dependence).
  projectIsPublic: false,
  projectHelpdeskEnabled: false,
  projectDeletedAt: null,
  projectArchivedAt: null,
  workspaceSlug: `acme`,
} as unknown as WidgetConfigWithProject

// A config whose support mode is fully live (helpdesk on, plan gate mocked
// green).
const supportConfig = {
  ...config,
  formConfig: { modes: [`feedback`, `support`] },
  projectHelpdeskEnabled: true,
} as unknown as WidgetConfigWithProject

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

  it(`solo workspace: auto-assigns the sole member, subscribes them as assignee, fires only issue_created`, async () => {
    h.getSoleHumanMemberId.mockResolvedValue(`member-1`)

    const result = await createWidgetSubmission({
      config,
      formData: submitForm(),
      userAgent: null,
    })

    expect(h.getSoleHumanMemberId).toHaveBeenCalledWith(`ws-1`)
    expect(issueInsert()?.values.assigneeId).toBe(`member-1`)
    expect(issueInsert()?.values.creatorId).toBe(`widget-bot`)

    expect(h.ensureSubscribed).toHaveBeenCalledTimes(1)
    expect(h.ensureSubscribed).toHaveBeenCalledWith(tx, {
      issueId: result.issueId,
      userId: `member-1`,
      workspaceId: `ws-1`,
      source: `assignee`,
    })

    expect(h.fireAndForgetNewIssueNotify).toHaveBeenCalledTimes(1)
    expect(h.fireAndForgetNewIssueNotify).toHaveBeenCalledWith({
      issueId: result.issueId,
    })
    // The auto-assignment must NOT double-notify via an "assigned you" row.
    expect(h.fireAndForgetAssignmentNotify).not.toHaveBeenCalled()
  })

  it(`multi-member workspace: no assignee, no assignee subscription, members still notified`, async () => {
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
    const requiredConfig: WidgetConfigWithProject = {
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

// EXP-130: the widget's support mode files a helpdesk ticket — issue +
// support thread + widget_reporter subscriber in one transaction, then the
// confirmation email carrying the magic link (emails are the token's only
// carrier — it is never stored).
describe(`createWidgetSupportSubmission`, () => {
  beforeEach(() => {
    h.inserts.length = 0
    h.dbInserts.length = 0
    h.txShouldFail = false
    h.getSoleHumanMemberId.mockClear()
    h.getSoleHumanMemberId.mockResolvedValue(null)
    h.ensureSubscribed.mockClear()
    h.fireAndForgetNewIssueNotify.mockClear()
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

  it(`files a ticket: issue + thread + reporter subscriber + confirmation email`, async () => {
    const result = await createWidgetSupportSubmission({
      config: supportConfig,
      formData: supportForm(),
      userAgent: `UA`,
    })

    const issue = h.inserts.find((i) => i.table === issues)
    expect(issue?.values.title).toBe(`My login is broken`)
    expect(issue?.values.description).toContain(`It loops back to the form.`)
    expect(issue?.values.creatorId).toBe(`widget-bot`)

    const subscriber = h.inserts.find((i) => i.table === issueSubscribers)
    expect(subscriber?.values.email).toBe(`reporter@example.com`)
    expect(subscriber?.values.source).toBe(`widget_reporter`)

    expect(h.createSupportThreadInTx).toHaveBeenCalledTimes(1)
    expect(h.createSupportThreadInTx.mock.calls[0][1]).toMatchObject({
      reporterEmail: `reporter@example.com`,
    })

    expect(h.fireAndForgetNewIssueNotify).toHaveBeenCalledTimes(1)
    expect(h.sendSupportConfirmationEmail).toHaveBeenCalledWith(
      expect.objectContaining({
        to: `reporter@example.com`,
        threadUrl: `https://app.test/support/tok-minted`,
      })
    )
    const delivery = h.dbInserts.find((i) => i.table === emailDeliveries)
    expect(delivery?.values.kind).toBe(`support_confirmation`)
    expect(delivery?.values.status).toBe(`sent`)

    // Support tickets never mint a public issue URL.
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

  it(`rejects when the target project's helpdesk is off`, async () => {
    const stale = {
      ...supportConfig,
      projectHelpdeskEnabled: false,
    } as unknown as WidgetConfigWithProject
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
    expect(result.identifier).toBe(`EXP-9`)
    expect(h.fireAndForgetNewIssueNotify).toHaveBeenCalledTimes(1)
  })

  it(`the feedback path refuses a support-only widget`, async () => {
    const supportOnly = {
      ...supportConfig,
      formConfig: { modes: [`support`] },
    } as unknown as WidgetConfigWithProject
    await expect(
      createWidgetSubmission({
        config: supportOnly,
        formData: submitForm(),
        userAgent: null,
      })
    ).rejects.toMatchObject({ status: 403 })
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
    } as unknown as WidgetConfigWithProject
    expect(requestedWidgetModes(junk)).toEqual([`support`])
  })

  it(`drops support when the project helpdesk is off, keeping feedback`, async () => {
    const stale = {
      ...supportConfig,
      projectHelpdeskEnabled: false,
    } as unknown as WidgetConfigWithProject
    expect(await effectiveWidgetModes(stale)).toEqual([`feedback`])
  })

  it(`a support-only widget degrades to feedback instead of a dead launcher`, async () => {
    h.assertCanUseHelpdesk.mockRejectedValue(new Error(`plan`))
    const supportOnly = {
      ...supportConfig,
      formConfig: { modes: [`support`] },
    } as unknown as WidgetConfigWithProject
    expect(await effectiveWidgetModes(supportOnly)).toEqual([`feedback`])
  })

  it(`serves both modes when everything is live`, async () => {
    expect(await effectiveWidgetModes(supportConfig)).toEqual([
      `feedback`,
      `support`,
    ])
  })
})

// EXP-162: a config may split its targets — feedback stays on project_id,
// support tickets file into support_project_id. The SUPPORT TARGET's
// helpdesk/trash state gates support; the primary's is irrelevant to it (and
// vice versa).
describe(`split support target (EXP-162)`, () => {
  // Primary helpdesk deliberately OFF — only the split target's flag counts.
  const splitConfig = {
    ...config,
    formConfig: { modes: [`feedback`, `support`] },
    projectHelpdeskEnabled: false,
    supportProjectId: `proj-support`,
    supportProjectHelpdeskEnabled: true,
    supportProjectDeletedAt: null,
  } as unknown as WidgetConfigWithProject

  beforeEach(() => {
    h.inserts.length = 0
    h.dbInserts.length = 0
    h.txShouldFail = false
    h.getSoleHumanMemberId.mockClear()
    h.getSoleHumanMemberId.mockResolvedValue(null)
    h.ensureSubscribed.mockClear()
    h.fireAndForgetNewIssueNotify.mockClear()
    h.assertCanUseHelpdesk.mockClear()
    h.assertCanUseHelpdesk.mockResolvedValue(undefined)
    h.createSupportThreadInTx.mockClear()
    h.sendSupportConfirmationEmail.mockClear()
  })

  const supportForm = (): FormData => {
    const form = new FormData()
    form.set(`mode`, `support`)
    form.set(`message`, `Where is my invoice?`)
    form.set(`email`, `reporter@example.com`)
    return form
  }

  it(`serves both modes off the split target's helpdesk flag`, async () => {
    expect(await effectiveWidgetModes(splitConfig)).toEqual([
      `feedback`,
      `support`,
    ])
  })

  it(`drops support when the split target's helpdesk is off`, async () => {
    const stale = {
      ...splitConfig,
      supportProjectHelpdeskEnabled: false,
      // Even with the PRIMARY helpdesk on — the split target decides.
      projectHelpdeskEnabled: true,
    } as unknown as WidgetConfigWithProject
    expect(await effectiveWidgetModes(stale)).toEqual([`feedback`])
  })

  it(`drops support when the split target is trashed`, async () => {
    const trashed = {
      ...splitConfig,
      supportProjectDeletedAt: new Date(),
    } as unknown as WidgetConfigWithProject
    expect(await effectiveWidgetModes(trashed)).toEqual([`feedback`])
  })

  it(`keeps serving support when only the FEEDBACK board is trashed`, async () => {
    // The config route must not hide the whole widget over a trashed primary
    // while a live split support target remains.
    const feedbackTrashed = {
      ...splitConfig,
      projectDeletedAt: new Date(),
    } as unknown as WidgetConfigWithProject
    expect(await effectiveWidgetModes(feedbackTrashed)).toEqual([`support`])
  })

  it(`returns NO modes when both targets are unavailable`, async () => {
    const allDead = {
      ...splitConfig,
      projectDeletedAt: new Date(),
      supportProjectDeletedAt: new Date(),
    } as unknown as WidgetConfigWithProject
    expect(await effectiveWidgetModes(allDead)).toEqual([])
  })

  it(`files the ticket into the support project, emailing the PRIMARY name`, async () => {
    await createWidgetSupportSubmission({
      config: splitConfig,
      formData: supportForm(),
      userAgent: null,
    })

    const issue = h.inserts.find((i) => i.table === issues)
    expect(issue?.values.projectId).toBe(`proj-support`)
    const subscriber = h.inserts.find((i) => i.table === issueSubscribers)
    expect(subscriber?.values.projectId).toBe(`proj-support`)
    expect(h.createSupportThreadInTx.mock.calls[0][1]).toMatchObject({
      projectId: `proj-support`,
    })
    // The product-facing identity stays the primary project's name — a
    // support project named "Support" must not render "Support support".
    expect(h.sendSupportConfirmationEmail).toHaveBeenCalledWith(
      expect.objectContaining({ projectName: `Board` })
    )
  })

  it(`falls back to the primary project when support_project_id is NULL`, async () => {
    await createWidgetSupportSubmission({
      config: supportConfig,
      formData: supportForm(),
      userAgent: null,
    })
    const issue = h.inserts.find((i) => i.table === issues)
    expect(issue?.values.projectId).toBe(`proj-1`)
  })

  it(`a trashed feedback board doesn't block support at a live split target`, async () => {
    const feedbackTrashed = {
      ...splitConfig,
      projectDeletedAt: new Date(),
    } as unknown as WidgetConfigWithProject

    const result = await createWidgetSupportSubmission({
      config: feedbackTrashed,
      formData: supportForm(),
      userAgent: null,
    })
    expect(result.identifier).toBe(`EXP-9`)

    // …while the feedback path still refuses the trashed primary board.
    await expect(
      createWidgetSubmission({
        config: feedbackTrashed,
        formData: submitForm(),
        userAgent: null,
      })
    ).rejects.toMatchObject({ status: 403 })
  })

  it(`a trashed support target rejects support despite a live primary`, async () => {
    const supportTrashed = {
      ...splitConfig,
      supportProjectDeletedAt: new Date(),
    } as unknown as WidgetConfigWithProject
    await expect(
      createWidgetSupportSubmission({
        config: supportTrashed,
        formData: supportForm(),
        userAgent: null,
      })
    ).rejects.toMatchObject({ status: 403 })
    expect(h.inserts).toHaveLength(0)
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

describe(`supportTicketTitle`, () => {
  it(`uses the first line`, () => {
    expect(supportTicketTitle(`Broken login\nmore detail`)).toBe(`Broken login`)
  })

  it(`clamps long first lines with an ellipsis`, () => {
    const title = supportTicketTitle(`x`.repeat(300))
    expect(title.length).toBeLessThanOrEqual(120)
    expect(title.endsWith(`…`)).toBe(true)
  })

  it(`falls back when the message starts blank`, () => {
    expect(supportTicketTitle(`\n\nactual text`)).toBe(`Support request`)
  })
})
