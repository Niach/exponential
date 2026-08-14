import { beforeEach, describe, expect, it, vi } from "vitest"

// EXP-496: locks the MCP bug-report intake to widget-submit parity — anonymous
// issue (`creator_id` NULL) with `source: agent`, solo-member auto-assign +
// subscribe, an email-only `widget_reporter` subscriber row for the resolution
// email, a widget_submissions row carrying the real reporter, and the
// post-commit issue_created fan-out.

const h = vi.hoisted(() => ({
  loadWidgetConfigByKey: vi.fn(),
  getSoleHumanMemberId: vi.fn(async (): Promise<string | null> => null),
  ensureSubscribed: vi.fn(),
  fireAndForgetNewIssueNotify: vi.fn(),
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
        // Awaited without .returning() (subscriber/submission inserts).
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
vi.mock(`@/lib/team-membership`, () => ({
  getSoleHumanMemberId: h.getSoleHumanMemberId,
}))
vi.mock(`@/lib/integrations/subscriptions`, () => ({
  ensureSubscribed: h.ensureSubscribed,
}))
vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetNewIssueNotify: h.fireAndForgetNewIssueNotify,
}))
// The real service.ts drags the email/helpdesk graph — mock the two exports
// agent-report.ts uses.
vi.mock(`@/lib/widget/service`, () => ({
  loadWidgetConfigByKey: h.loadWidgetConfigByKey,
  WidgetRequestError: class WidgetRequestError extends Error {
    constructor(
      readonly status: number,
      message: string
    ) {
      super(message)
    }
  },
}))

import { issues, issueSubscribers, widgetSubmissions } from "@/db/schema"
import { createAgentBugReport } from "@/lib/widget/agent-report"
import { WidgetRequestError } from "@/lib/widget/service"

const config = {
  id: `cfg-1`,
  teamId: `ws-1`,
  boardId: `proj-1`,
  publicKey: `expw_test`,
  boardDeletedAt: null,
}

const args = {
  widgetKey: `expw_test`,
  reporter: { email: `dev@example.com`, name: `Dev` },
  title: `Sync loop stuck`,
  description: `Steps: open a board…`,
  userAgent: `claude-code/test`,
}

function insertsInto(table: unknown) {
  return h.inserts.filter((insert) => insert.table === table)
}

beforeEach(() => {
  vi.clearAllMocks()
  h.inserts.length = 0
  h.txShouldFail = false
  h.loadWidgetConfigByKey.mockResolvedValue(config)
  h.getSoleHumanMemberId.mockResolvedValue(null)
})

describe(`createAgentBugReport`, () => {
  it(`files an anonymous agent-sourced issue with the reporter recorded`, async () => {
    h.getSoleHumanMemberId.mockResolvedValue(`member-1`)

    const result = await createAgentBugReport(args)

    expect(result).toEqual({ issueId: `generated`, identifier: `EXP-9` })
    expect(h.loadWidgetConfigByKey).toHaveBeenCalledWith(`expw_test`)

    const [issueInsert] = insertsInto(issues)
    expect(issueInsert.values).toEqual({
      boardId: `proj-1`,
      teamId: `ws-1`,
      title: `Sync loop stuck`,
      status: `backlog`,
      priority: `none`,
      description: `Steps: open a board…`,
      assigneeId: `member-1`,
      creatorId: null,
      source: `agent`,
    })

    // Solo-member auto-assign subscribes like issues.create does.
    expect(h.ensureSubscribed).toHaveBeenCalledWith(tx, {
      issueId: `generated`,
      userId: `member-1`,
      teamId: `ws-1`,
      source: `assignee`,
    })

    // The reporter rides the widget_reporter rails (resolution email).
    const [subscriberInsert] = insertsInto(issueSubscribers)
    expect(subscriberInsert.values).toEqual({
      issueId: `generated`,
      userId: null,
      email: `dev@example.com`,
      teamId: `ws-1`,
      boardId: `proj-1`,
      source: `widget_reporter`,
      unsubscribed: false,
    })

    const [submissionInsert] = insertsInto(widgetSubmissions)
    expect(submissionInsert.values).toEqual({
      widgetConfigId: `cfg-1`,
      issueId: `generated`,
      reporterEmail: `dev@example.com`,
      reporterName: `Dev`,
      userAgent: `claude-code/test`,
      customData: { via: `mcp` },
    })

    expect(h.fireAndForgetNewIssueNotify).toHaveBeenCalledWith({
      issueId: `generated`,
    })
  })

  it(`skips assignment when the team has no sole human member`, async () => {
    await createAgentBugReport(args)

    const [issueInsert] = insertsInto(issues)
    expect(issueInsert.values.assigneeId).toBeNull()
    expect(h.ensureSubscribed).not.toHaveBeenCalled()
  })

  it(`rejects when the feedback board is missing or trashed`, async () => {
    for (const broken of [
      { ...config, boardId: null },
      { ...config, boardDeletedAt: new Date() },
    ]) {
      h.loadWidgetConfigByKey.mockResolvedValue(broken)
      const error = await createAgentBugReport(args).catch((e) => e)
      expect(error).toBeInstanceOf(WidgetRequestError)
      expect((error as { status: number }).status).toBe(403)
    }
    expect(h.inserts).toHaveLength(0)
    expect(h.fireAndForgetNewIssueNotify).not.toHaveBeenCalled()
  })

  it(`does not notify when the transaction fails`, async () => {
    h.txShouldFail = true
    await expect(createAgentBugReport(args)).rejects.toThrow(`TX_FAILED`)
    expect(h.fireAndForgetNewIssueNotify).not.toHaveBeenCalled()
  })
})
