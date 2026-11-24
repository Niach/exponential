import { atom, read, update } from 'claude-code'
import type { EngineInterface, Register } from 'claude-code'

import type { Issue, ListRow, View } from '../types'
import {
  PRIORITY_LABEL,
  SESSION_STATE_GLYPH,
  SESSION_STATE_LABEL,
  SESSION_STATE_TONE,
  STATUS_GLYPH,
  STATUS_LABEL,
  STATUS_TONE,
  TONE_COLOR,
  branchIssueIdentifier,
  clip,
  freshNotifications,
  getIssue,
  groupIssuesByStatus,
  isPullRequestWrite,
  issueStatusLine,
  leadingPullRequest,
  listIssues,
  listUnreadNotifications,
  newestMark,
  normalizePriority,
  normalizeStatus,
  notificationToast,
  parseIssueIdentifier,
  pullRequestLine,
  pullRequestToast,
  pullRequestTone,
  sessionState,
  type CallTool,
  type IssueRow,
  type SeenMark,
} from './kit/index'

// Exponential in Claude Code (#NEW-23). Claude Code draws no MCP Apps views,
// so the mods draw the same data with the surface's own elements: a pane
// with the issue of the current branch (its pull request and live sessions),
// a status line that follows the pull request, and a toast when a comment, a
// mention or a waiting session arrives. Every word and colour comes from
// `./kit` (generated from packages/plugin-kit), never from here.

const PANE = 'exponential'
const SERVER = 'exponential'
const ISSUE_POLL_MS = 30_000
const INBOX_POLL_MS = 60_000
const SEEN_KEY = 'inbox.seen'

const view = atom({ plugin: 'exponential-code', key: 'view' } as const, 'issue' as View)
const pinned = atom({ plugin: 'exponential-code', key: 'pinned' } as const, null)
const issue = atom({ plugin: 'exponential-code', key: 'issue' } as const, null)
const list = atom({ plugin: 'exponential-code', key: 'list' } as const, null)
const note = atom({ plugin: 'exponential-code', key: 'note' } as const, null)

const START_PROMPT = (identifier: string) =>
  `Start a coding agent on #${identifier} in Exponential: read the issue with exponential_issues {op: "get"}, list my devices with exponential_devices {op: "list"}, ask me which one when more than one is online, then start the session with exponential_sessions_send {teamId, deviceId, issueIds: ["${identifier}"]} and give me its link.`

// Module variables start over on a reload; what a drawing reads is in $.state.
let server = SERVER
let cwd: string | undefined
let timers: { cancel: () => void }[] = []
let isPolling = false

function transport($: EngineInterface): CallTool {
  return (tool, args) => $.mcp.call(server, tool, args)
}

async function branchIdentifier($: EngineInterface): Promise<string | null> {
  try {
    const ran = await $.process.run(['git', 'branch', '--show-current'], { ...(cwd ? { cwd } : {}), timeoutMs: 3000 })
    return ran.exitCode === 0 ? branchIssueIdentifier(ran.stdout.trim()) : null
  } catch {
    return null
  }
}

/** The pinned issue, else the one the branch names. */
async function currentIdentifier($: EngineInterface): Promise<string | null> {
  return (await read($, pinned)) ?? (await branchIdentifier($))
}

/** Re-reads the current issue; the status line follows it. */
async function refresh($: EngineInterface): Promise<Issue | null> {
  const identifier = await currentIdentifier($)
  if (!identifier) {
    await update($, issue, () => null)
    $.ui.status(undefined)
    return null
  }
  const got = await getIssue(transport($), identifier)
  if (got.kind === 'error') {
    // Not an issue of this instance, or not signed in: say it in the pane
    // only, never as a toast on every poll.
    await update($, issue, () => null)
    await update($, note, () => got.message)
    $.ui.status(undefined)
    return null
  }
  const row: Issue = got.data
  await update($, issue, () => row)
  await update($, note, () => null)
  $.ui.status(issueStatusLine(row.identifier, row.pullRequests))
  return row
}

async function loadList($: EngineInterface): Promise<void> {
  const got = await listIssues(transport($))
  if (got.kind === 'error') {
    await update($, note, () => got.message)
    return
  }
  const rows: ListRow[] = got.data.map(row => ({
    id: row.id,
    identifier: row.identifier,
    title: row.title,
    status: row.status,
    priority: row.priority,
  }))
  await update($, list, () => rows)
  await update($, note, () => null)
}

/** Toasts what arrived since the newest row any session on this machine saw. */
async function pollInbox($: EngineInterface): Promise<void> {
  if (isPolling) return
  isPolling = true
  try {
    await readInbox($)
  } finally {
    isPolling = false
  }
}

async function readInbox($: EngineInterface): Promise<void> {
  const got = await listUnreadNotifications(transport($))
  if (got.kind === 'error') return
  const seen = ((await $.store.get(SEEN_KEY)) as SeenMark | undefined) ?? null
  const newest = newestMark(got.data)
  if (newest && (!seen || newest.createdAt > seen.createdAt)) await $.store.set(SEEN_KEY, newest)
  const current = await read($, issue)
  for (const row of freshNotifications(got.data, seen, current?.id)) {
    $.ui.toast(notificationToast(row), { timeoutMs: 8000 })
  }
}

async function showIssue($: EngineInterface, identifier: string): Promise<Issue | null> {
  await update($, pinned, () => identifier)
  await update($, view, () => 'issue')
  return refresh($)
}

export const register: Register = on => {
  on('session.start', async ($, e, next) => {
    cwd = e.cwd
    const connected = await $.mcp.connect(SERVER).catch(() => undefined)
    if (connected?.isConnected) server = connected.server

    await $.command.register({ name: 'exp-issues', description: 'Open issues in Exponential, in a pane' })
    await $.command.register({
      name: 'exp-issue',
      description: 'Show an Exponential issue with its pull request and sessions',
      argumentHint: '[EXP-12]',
    })
    await $.command.register({
      name: 'exp-start',
      description: 'Start a coding agent on an Exponential issue',
      argumentHint: '[EXP-12]',
    })

    for (const timer of timers) timer.cancel()
    timers = [$.clock.every(ISSUE_POLL_MS, () => void refresh($)), $.clock.every(INBOX_POLL_MS, () => void pollInbox($))]

    // The first reads never hold the session's start.
    void refresh($).then(row => {
      if (row) void $.ui.open({ id: PANE, title: row.identifier })
    })
    void pollInbox($)

    return next(e)
  })

  on('command.run', { command: 'exp-issues' }, async $ => {
    await update($, view, () => 'list')
    await loadList($)
    await $.ui.open({ id: PANE, title: 'Exponential' })
    const rows = (await read($, list)) ?? []

    return { text: rows.length === 1 ? '1 open issue' : `${rows.length} open issues` }
  })

  on('command.run', { command: 'exp-issue' }, async ($, e) => {
    const identifier = parseIssueIdentifier(e.args) ?? (await currentIdentifier($))
    if (!identifier) return { text: 'Name an issue: /exp-issue EXP-12' }
    const row = await showIssue($, identifier)
    if (!row) return { text: (await read($, note)) ?? `${identifier} was not found` }
    await $.ui.open({ id: PANE, title: row.identifier })

    return { text: `${issueStatusLine(row.identifier, row.pullRequests)} · ${clip(row.title, 80)}` }
  })

  on('command.run', { command: 'exp-start' }, async ($, e) => {
    const identifier = parseIssueIdentifier(e.args) ?? (await currentIdentifier($))
    if (!identifier) return { text: 'Name an issue: /exp-start EXP-12' }
    // The person sends it: a command's hook holds the turn a submit would wait on.
    await $.prompt.fill({ text: START_PROMPT(identifier) })

    return { text: `Press Enter to start a coding agent on ${identifier}` }
  })

  // A PR opened or queued from this session: the status line follows at once.
  on('tool.call', async ($, e, next) => {
    const ran = await next(e)
    if (!isPullRequestWrite(String(e.tool)) || ran.deny !== undefined || ran.isError) return ran
    const op = (e as { op?: unknown }).op
    if (op !== 'open' && op !== 'merge') return ran
    try {
      const reply = JSON.parse(ran.text ?? '') as { number?: number; url?: string }
      const text = pullRequestToast(op === 'open' ? 'opened' : 'queued', reply)
      if (text) $.ui.toast(text, { timeoutMs: 8000 })
    } catch {
      // A reply that is not JSON says nothing to toast.
    }
    void refresh($)

    return ran
  }).catch((_, e, next) => next(e))

  on('ui.render', { component: 'Pane', requestId: PANE }, async ($, e) => {
    const { Box, Text, Button, Link } = $.ui.resolve(e)
    const columns = Math.max(20, e.props.bodyColumns ?? 40)
    const shown = await read($, view)
    const message = await read($, note)

    if (shown === 'list') {
      const rows = await read($, list)
      const groups = groupIssuesByStatus(
        (rows ?? []).map(
          (row): IssueRow => ({ ...row, status: normalizeStatus(row.status), priority: normalizePriority(row.priority) }),
        ),
      )

      return (
        <Box flexDirection="column">
          {message !== null && <Text dimColor>{clip(message, columns)}</Text>}
          {rows === null && message === null && <Text dimColor>Loading</Text>}
          {rows !== null && rows.length === 0 && <Text dimColor>No open issues</Text>}
          {groups.map(group => (
            <Box flexDirection="column" marginBottom={1}>
              <Text color={TONE_COLOR[STATUS_TONE[group.status]]}>
                {STATUS_GLYPH[group.status]} {STATUS_LABEL[group.status]} <Text dimColor>{group.issues.length}</Text>
              </Text>
              {group.issues.map(row => (
                <Button key={row.identifier} plain onPress={() => void showIssue($, row.identifier)}>
                  {row.identifier} <Text dimColor>{clip(row.title, columns - row.identifier.length - 3)}</Text>
                </Button>
              ))}
            </Box>
          ))}
        </Box>
      )
    }

    const row = await read($, issue)
    if (row === null) {
      return (
        <Box flexDirection="column">
          <Text dimColor>{clip(message ?? 'No issue on this branch', columns)}</Text>
          <Button
            key="list"
            label="Open issues"
            onPress={async () => {
              await update($, view, () => 'list')
              await loadList($)
            }}
          />
        </Box>
      )
    }

    const status = normalizeStatus(row.status)
    const priority = normalizePriority(row.priority)
    const pr = leadingPullRequest(row.pullRequests)

    return (
      <Box flexDirection="column">
        <Text>
          <Text color={TONE_COLOR[STATUS_TONE[status]]}>{STATUS_GLYPH[status]}</Text> <Text bold>{row.identifier}</Text>{' '}
          <Text dimColor>
            {STATUS_LABEL[status]}
            {priority === 'none' ? '' : ` · ${PRIORITY_LABEL[priority]}`}
          </Text>
        </Text>
        <Text>{row.title}</Text>
        <Box marginTop={1} flexDirection="column">
          {pr === null ? (
            <Text dimColor>{pullRequestLine(null)}</Text>
          ) : (
            <Text color={TONE_COLOR[pullRequestTone(pr)]}>{pullRequestLine(pr)}</Text>
          )}
          {(row.sessions ?? []).map(session => {
            const state = sessionState(session)

            return (
              <Text>
                <Text color={TONE_COLOR[SESSION_STATE_TONE[state]]}>{SESSION_STATE_GLYPH[state]}</Text> Session{' '}
                <Text dimColor>{SESSION_STATE_LABEL[state]}</Text>
              </Text>
            )
          })}
        </Box>
        <Box marginTop={1} flexDirection="column">
          {pr !== null && <Link href={pr.url}>Pull request</Link>}
          {typeof row.url === 'string' && <Link href={row.url}>Open in Exponential</Link>}
          <Button
            key="list"
            label="Open issues"
            onPress={async () => {
              await update($, view, () => 'list')
              await loadList($)
            }}
          />
        </Box>
      </Box>
    )
  })
}
