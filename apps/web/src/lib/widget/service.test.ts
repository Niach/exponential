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
  inserts: [] as Array<{ table: unknown; values: Record<string, unknown> }>,
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
  },
}))

// lib/trpc.ts imports `auth`/db at module scope; only generateTxId is used.
vi.mock(`@/lib/trpc`, () => ({ generateTxId: vi.fn(async () => 1) }))
vi.mock(`@/lib/billing`, () => ({
  assertWithinStorageLimit: vi.fn(async () => undefined),
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

import { issues, issueSubscribers } from "@/db/schema"
import {
  createWidgetSubmission,
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
  // `tasks` keeps publicIssueUrl() out of play (no appBaseUrl dependence).
  projectType: `tasks`,
  projectDeletedAt: null,
  projectArchivedAt: null,
  workspaceSlug: `acme`,
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
