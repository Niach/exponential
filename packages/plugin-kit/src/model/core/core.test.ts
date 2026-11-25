import { readFileSync, readdirSync } from "node:fs"
import { join } from "node:path"
import { REPO_ROOT } from "../../generate"
import {
  ISSUE_PRIORITIES,
  ISSUE_STATUSES,
  SESSION_STATE_LABEL,
  STATUS_TONE,
  TONE_COLOR,
  TOAST_NOTIFICATION_TYPES,
  branchIssueIdentifier,
  clip,
  freshNotifications,
  getIssue,
  issueStatusLine,
  leadingPullRequest,
  leadingSession,
  listIssues,
  listUnreadNotifications,
  newestMark,
  notificationToast,
  parseIssueIdentifier,
  pullRequestToast,
  sessionState,
  type IssuePullRequest,
  type NotificationRow,
} from "./index"

const contract = JSON.parse(readFileSync(join(REPO_ROOT, `packages/domain-contract/contract.json`), `utf8`))

describe(`the dependency-free core`, () => {
  it(`imports nothing outside its folder`, () => {
    const dir = join(REPO_ROOT, `packages/plugin-kit/src/model/core`)
    for (const file of readdirSync(dir).filter((name) => name.endsWith(`.ts`) && !name.endsWith(`.test.ts`))) {
      const imports = [...readFileSync(join(dir, file), `utf8`).matchAll(/from "([^"]+)"/g)].map((match) => match[1]!)
      expect(imports.filter((path) => !/^\.\/[a-z-]+$/.test(path)), file).toEqual([])
    }
  })

  it(`keeps the contract's enums`, () => {
    expect([...ISSUE_STATUSES]).toEqual(contract.issueStatus.values)
    expect([...ISSUE_PRIORITIES].sort()).toEqual([...contract.issuePriority.values].sort())
    for (const type of TOAST_NOTIFICATION_TYPES) expect(contract.notificationType.values).toContain(type)
    for (const tone of Object.values(STATUS_TONE)) expect(TONE_COLOR[tone]).toMatch(/^#[0-9A-F]{6}$/i)
  })
})

describe(`the issue a branch names`, () => {
  it(`reads an identifier anywhere in the branch`, () => {
    expect(branchIssueIdentifier(`exp/EXP-12`)).toBe(`EXP-12`)
    expect(branchIssueIdentifier(`feature/new-23-plugin`)).toBe(`NEW-23`)
    expect(branchIssueIdentifier(`EXP-2107`)).toBe(`EXP-2107`)
  })

  it(`reads none from a session branch or a plain one`, () => {
    expect(branchIssueIdentifier(`exp/session-b0291ed2`)).toBeNull()
    expect(branchIssueIdentifier(`master`)).toBeNull()
    expect(branchIssueIdentifier(`release-2026`)).toBeNull()
    expect(branchIssueIdentifier(null)).toBeNull()
  })

  it(`parses what a person types`, () => {
    expect(parseIssueIdentifier(` #exp-12 `)).toBe(`EXP-12`)
    expect(parseIssueIdentifier(`fix the bug`)).toBeNull()
    expect(clip(`a  long\ntitle here`, 9)).toBe(`a long t…`)
  })
})

const pr = (over: Partial<IssuePullRequest> = {}): IssuePullRequest => ({
  pullRequestId: `p1`,
  number: 12,
  url: `https://github.com/acme/app/pull/12`,
  state: `open`,
  status: `Checks running`,
  checks: `pending`,
  ...over,
})

describe(`the status line`, () => {
  it(`names the issue and where its PR stands`, () => {
    expect(issueStatusLine(`EXP-12`, [])).toBe(`EXP-12 · No PR`)
    expect(issueStatusLine(`EXP-12`, undefined)).toBe(`EXP-12 · No PR`)
    expect(issueStatusLine(`EXP-12`, [pr()])).toBe(`EXP-12 · PR #12 · Checks running`)
    expect(issueStatusLine(`EXP-12`, [pr({ status: `Queued · 2nd` })])).toBe(`EXP-12 · PR #12 · Queued · 2nd`)
  })

  it(`leads with the live PR, else the newest`, () => {
    const merged = pr({ number: 14, state: `merged`, status: `Merged` })
    expect(leadingPullRequest([merged, pr()])?.number).toBe(12)
    expect(leadingPullRequest([merged, pr({ number: 9, state: `closed` })])?.number).toBe(14)
  })

  it(`words a PR tool's reply`, () => {
    expect(pullRequestToast(`opened`, { number: 12, url: `https://x/12` })).toBe(`PR #12 opened · https://x/12`)
    expect(pullRequestToast(`queued`, {})).toBeNull()
  })
})

describe(`a session's state`, () => {
  it(`follows session-display: needs input beats working`, () => {
    expect(sessionState({ status: `running`, busy: true, needsInput: true })).toBe(`needs_input`)
    expect(sessionState({ status: `running`, busy: true, needsInput: false })).toBe(`working`)
    expect(sessionState({ status: `queued`, busy: false, needsInput: false })).toBe(`queued`)
    expect(SESSION_STATE_LABEL[sessionState({ status: `idle`, busy: false, needsInput: false })]).toBe(`Idle`)
  })

  it(`leads with the one that waits on a person`, () => {
    const rows = [
      { sessionId: `a`, rootId: `a`, status: `idle`, busy: false, needsInput: false },
      { sessionId: `b`, rootId: `b`, status: `running`, busy: true, needsInput: false },
      { sessionId: `c`, rootId: `c`, status: `running`, busy: false, needsInput: true },
    ]
    expect(leadingSession(rows)?.sessionId).toBe(`c`)
    expect(leadingSession(rows.slice(0, 2))?.sessionId).toBe(`b`)
    expect(leadingSession([])).toBeNull()
  })
})

const note = (id: string, minute: number, over: Partial<NotificationRow> = {}): NotificationRow => ({
  id,
  issueId: `i1`,
  type: `issue_comment`,
  title: `Mia commented on EXP-12`,
  body: `Looks good`,
  readAt: null,
  createdAt: `2026-11-24T10:${String(minute).padStart(2, `0`)}:00.000Z`,
  ...over,
})

describe(`inbox toasts`, () => {
  it(`toasts nothing on the first poll, then only what is newer`, () => {
    const first = [note(`b`, 2), note(`a`, 1)]
    expect(freshNotifications(first, null)).toEqual([])
    const seen = newestMark(first)
    const next = [note(`d`, 4), note(`c`, 3, { type: `issue_status_changed` }), ...first]
    expect(freshNotifications(next, seen).map((row) => row.id)).toEqual([`d`])
  })

  it(`narrows to the current issue but keeps a waiting session`, () => {
    const seen = { id: `a`, createdAt: note(`a`, 1).createdAt }
    const rows = [note(`d`, 4, { issueId: null, type: `session_blocked` }), note(`c`, 3, { issueId: `i2` }), note(`b`, 2), note(`a`, 1)]
    expect(freshNotifications(rows, seen, `i1`).map((row) => row.id)).toEqual([`b`, `d`])
    expect(freshNotifications(rows, seen).map((row) => row.id)).toEqual([`b`, `c`, `d`])
  })

  it(`skips a row someone already read`, () => {
    const seen = { id: `a`, createdAt: note(`a`, 1).createdAt }
    expect(freshNotifications([note(`b`, 2, { readAt: `2026-11-24T10:03:00.000Z` })], seen)).toEqual([])
  })

  it(`says the row's own words`, () => {
    expect(notificationToast(note(`a`, 1))).toBe(`Mia commented on EXP-12 · Looks good`)
    expect(notificationToast(note(`a`, 1, { body: null }))).toBe(`Mia commented on EXP-12`)
  })
})

describe(`the typed callers`, () => {
  const text = (value: unknown) => ({ content: [{ type: `text`, text: typeof value === `string` ? value : JSON.stringify(value) }] })

  it(`reads an issue without its comments`, async () => {
    const calls: unknown[] = []
    const got = await getIssue(async (tool, args) => (calls.push([tool, args]), text({ id: `i1`, identifier: `EXP-12` })), `EXP-12`)
    expect(calls).toEqual([[`exponential_issues`, { op: `get`, id: `EXP-12`, commentsLimit: 0 }]])
    expect(got).toEqual({ kind: `ok`, data: { id: `i1`, identifier: `EXP-12` } })
  })

  it(`reads a list from the view's structured result or a plain array`, async () => {
    const row = { id: `i1`, identifier: `EXP-12` }
    const structured = async () => ({ ...text(`1 issue (1–1), shown to the person in the Exponential view.`), structuredContent: { items: [row], page: {} } })
    expect(await listIssues(structured)).toEqual({ kind: `ok`, data: [row] })
    expect(await listUnreadNotifications(async () => text([row]))).toEqual({ kind: `ok`, data: [row] })
    // A host that drops the structured result leaves only the summary.
    expect(await listIssues(async () => text(`1 issue (1–1), shown to the person in the Exponential view.`))).toEqual({ kind: `error`, message: `The list could not be read.` })
  })

  it(`answers a transport failure as an error, never a throw`, async () => {
    expect(await listIssues(async () => Promise.reject(new Error(`not connected`)))).toEqual({ kind: `error`, message: `not connected` })
    expect(await getIssue(async () => ({ isError: true, content: [{ type: `text`, text: `Issue not found` }] }), `X-1`)).toEqual({ kind: `error`, message: `Issue not found` })
  })
})
