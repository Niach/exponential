import { expect, mock, test } from 'claude-code/testing'
import type { On } from 'claude-code'

// The three mods against the engine itself (`claude plugin test`): every hook
// below stands for the engine beneath the plugin, the MCP server included.

const ISSUE = {
  id: 'i-12',
  identifier: 'EXP-12',
  title: 'The export drops the last row',
  status: 'in_progress',
  priority: 'high',
  url: 'https://app.exponential.at/t/acme/boards/app/issues/EXP-12',
  pullRequests: [
    { pullRequestId: 'p-1', number: 40, url: 'https://github.com/acme/app/pull/40', state: 'open', status: 'Checks running', checks: 'pending' },
  ],
  sessions: [{ sessionId: 's-1', rootId: 's-1', status: 'running', busy: true, needsInput: false }],
}

const LIST = [
  { id: 'i-12', identifier: 'EXP-12', title: 'The export drops the last row', status: 'in_progress', priority: 'high' },
  { id: 'i-13', identifier: 'EXP-13', title: 'Dark mode flickers', status: 'backlog', priority: 'none' },
]

const note = (id: string, minute: number, type = 'issue_comment') => ({
  id,
  issueId: 'i-12',
  type,
  title: 'Mia commented on EXP-12',
  body: 'Looks good',
  readAt: null,
  createdAt: `2026-11-24T10:0${minute}:00.000Z`,
})

const PANE = {
  plugin: 'exponential-code',
  component: 'Pane',
  requestId: 'exponential',
  props: {
    title: 'EXP-12',
    isFocused: false,
    bodyColumns: 48,
    placement: 'dock',
    scroll: { offset: 0, bodyRows: 30 },
    view: {},
  },
} as const

type World = {
  clock: ReturnType<typeof mock.clock>
  branch: string
  issue: typeof ISSUE | null
  inbox: ReturnType<typeof note>[]
  status: (string | undefined)[]
  toasts: string[]
  opened: string[]
  calls: { tool: string; args: Record<string, unknown> }[]
  submitted: string[]
}

const reply = (data: unknown) => ({ value: { content: [{ type: 'text', text: JSON.stringify(data) }] } })

/** The engine and the Exponential server beneath the plugin. */
function world(on: On, branch = 'exp/EXP-12'): World {
  const state: World = { clock: mock.clock(on), branch, issue: ISSUE, inbox: [], status: [], toasts: [], opened: [], calls: [], submitted: [] }
  mock.store(on)
  on('session.start', (_, e) => ({ cwd: e.cwd }))
  on('command.register', (_, e) => ({ value: { command: e.name } }))
  on('mcp.connect', () => ({ value: { isConnected: true, server: 'plugin:exponential-code:exponential' } }))
  on('process.run', () => ({ value: { exitCode: 0, stdout: `${state.branch}\n`, stderr: '' } }))
  on('ui.open', (_, e) => {
    state.opened.push(e.title ?? '')

    return { value: { isPlaced: true } }
  })
  on('ui.status', (_, e) => {
    state.status.push(e.text)

    return { value: undefined }
  })
  on('ui.toast', (_, e) => {
    state.toasts.push(e.text)

    return { value: undefined }
  })
  on('prompt.fill', (_, e) => {
    state.submitted.push(e.text)

    return { isFilled: true }
  })
  on('mcp.call', (_, e) => {
    state.calls.push({ tool: e.tool, args: e.args ?? {} })
    if (e.tool === 'exponential_notifications') return reply(state.inbox)
    if (e.args?.op === 'list') return reply(LIST)
    if (state.issue === null) return { value: { isError: true, content: [{ type: 'text', text: 'Issue not found' }] } }

    return reply(state.issue)
  })

  return state
}

const START = { cwd: '/repo', surface: 'terminal', isInteractive: true } as const

/** Starts the session and lets its first reads (never awaited by the start) land. */
async function start($: { session: { start: (input: typeof START) => Promise<unknown> } }, w: World): Promise<void> {
  await $.session.start(START)
  await w.clock.settle()
}

test('the status line names the branch issue and where its PR stands', async ($, on) => {
  const w = world(on)
  await start($, w)

  expect(w.status.at(-1)).toBe('EXP-12 · PR #40 · Checks running')
  expect(w.calls).toContainEqual({ tool: 'exponential_issues', args: { op: 'get', id: 'EXP-12', commentsLimit: 0 } })
  expect(w.opened).toEqual(['EXP-12'])

  // It follows the PR on the next poll.
  w.issue = { ...ISSUE, pullRequests: [{ ...ISSUE.pullRequests[0]!, status: 'Queued · 2nd' }] }
  await w.clock.advance(30_000)
  expect(w.status.at(-1)).toBe('EXP-12 · PR #40 · Queued · 2nd')
})

test('a branch that names no issue shows no status line', async ($, on) => {
  const w = world(on, 'exp/session-b0291ed2')
  await start($, w)
  const ran = await $.command.run({ command: 'exp-issue', args: '' })

  expect(ran).toEqual({ text: 'Name an issue: /exp-issue EXP-12' })
  expect(w.status.filter(text => text !== undefined)).toEqual([])
  expect(w.calls.filter(call => call.tool === 'exponential_issues')).toEqual([])
})

test('the pane draws the issue, its PR and its session on every surface', async ($, on) => {
  const w = world(on)
  await start($, w)
  await $.command.run({ command: 'exp-issue', args: 'exp-12' })

  for (const surface of ['terminal', 'desktop', 'vscode', 'mobile'] as const) {
    const ui = await $.ui.mount({ ...PANE, surface })

    expect(await ui.find({ type: 'Text', text: /The export drops the last row/ })).toBeDefined()
    expect(await ui.find({ type: 'Text', text: /In Progress · High/ })).toBeDefined()
    expect((await ui.find({ type: 'Text', text: /PR #40/ }))?.text).toBe('PR #40 · Checks running')
    expect(await ui.find({ type: 'Text', text: /Working/ })).toBeDefined()
    await ui.unmount()
  }
})

test('/exp-issues lists the open issues and a row opens its issue', async ($, on) => {
  const w = world(on, 'master')
  await start($, w)
  const ran = await $.command.run({ command: 'exp-issues', args: '' })
  expect(ran).toEqual({ text: '2 open issues' })

  for (const surface of ['terminal', 'desktop'] as const) {
    await $.command.run({ command: 'exp-issues', args: '' })
    const ui = await $.ui.mount({ ...PANE, surface })
    expect(await ui.find({ type: 'Text', text: /In Progress/ })).toBeDefined()
    expect(await ui.find({ type: 'Text', text: /Dark mode flickers/ })).toBeDefined()

    await ui.press({ key: 'EXP-12' })
    expect(await ui.find({ type: 'Text', text: /PR #40 · Checks running/ })).toBeDefined()
    expect(w.status.at(-1)).toBe('EXP-12 · PR #40 · Checks running')
    await ui.unmount()
  }
})

test('an issue the instance does not know leaves the status line empty', async ($, on) => {
  const w = world(on)
  w.issue = null
  await start($, w)
  const ran = await $.command.run({ command: 'exp-issue', args: 'EXP-99' })

  expect(ran).toEqual({ text: 'Issue not found' })
  expect(w.status.at(-1)).toBeUndefined()
  expect(w.toasts).toEqual([])
})

test('a PR opened in this session toasts its link and refreshes the status line', async ($, on) => {
  const w = world(on)
  on('tool.call', () => ({ result: {}, text: JSON.stringify({ number: 41, url: 'https://github.com/acme/app/pull/41' }) }))
  await $.session.start(START)
  await $.command.run({ command: 'exp-issue', args: '' })
  const before = w.calls.length

  await $.tool.call({ tool: 'mcp__plugin_exponential-code_exponential__exponential_pr_write', op: 'open', title: 'Fix' })
  expect(w.toasts).toEqual(['PR #41 opened · https://github.com/acme/app/pull/41'])
  await $.command.run({ command: 'exp-issue', args: '' })
  expect(w.calls.length).toBeGreaterThan(before)

  await $.tool.call({ tool: 'Bash', command: 'ls' })
  await $.tool.call({ tool: 'mcp__plugin_exponential-code_exponential__exponential_pr_write', op: 'update' })
  await w.clock.settle()
  expect(w.toasts.length).toBe(1)
})

test('a new comment toasts once; the first poll only remembers the newest row', async ($, on) => {
  const w = world(on)
  w.inbox = [note('n-1', 1)]
  await start($, w)
  expect(w.toasts).toEqual([])

  w.inbox = [note('n-3', 3, 'issue_status_changed'), note('n-2', 2), note('n-1', 1)]
  await w.clock.advance(60_000)
  expect(w.toasts).toEqual(['Mia commented on EXP-12 · Looks good'])

  await w.clock.advance(60_000)
  expect(w.toasts.length).toBe(1)
})

test('/exp-start hands the model the issue to start an agent on', async ($, on) => {
  const w = world(on)
  await start($, w)
  const ran = await $.command.run({ command: 'exp-start', args: '#exp-12' })

  expect(ran).toEqual({ text: 'Press Enter to start a coding agent on EXP-12' })
  expect(w.submitted[0]).toContain('issueIds: ["EXP-12"]')
})
