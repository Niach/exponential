/* ─── IDE playground fixture data — dogfood: Exponential building Exponential ─── */

export type IssueStatus = `backlog` | `todo` | `in_progress` | `done`
export type IssuePriority = `none` | `urgent` | `high` | `medium` | `low`
export type FilterTab = `all` | `active` | `backlog`

export type Assignee = { initials: string; name: string }
export type Label = { name: string; color: string }

export type Issue = {
  id: string
  title: string
  status: IssueStatus
  priority: IssuePriority
  assignee?: Assignee
  labels?: Label[]
  due?: string
}

export const DS: Assignee = { initials: `DS`, name: `Danny Strähhuber` }

export const PROJECT = { name: `Exponential`, color: `#4f46e5` }

export const ISSUES: Issue[] = [
  {
    id: `EXP-8`,
    title: `Live-steer terminal reconnect`,
    status: `in_progress`,
    priority: `high`,
    assignee: DS,
    due: `Jul 9`,
  },
  {
    id: `EXP-11`,
    title: `Issue board keyboard navigation`,
    status: `todo`,
    priority: `medium`,
    due: `Jul 15`,
  },
  {
    id: `EXP-12`,
    title: `Attachment paste uploads`,
    status: `todo`,
    priority: `none`,
  },
  {
    id: `EXP-9`,
    title: `Recurring issues UI polish`,
    status: `backlog`,
    priority: `none`,
  },
  {
    id: `EXP-13`,
    title: `Widget screenshot annotations`,
    status: `backlog`,
    priority: `none`,
    labels: [{ name: `feedback`, color: `#22c55e` }],
  },
  {
    id: `EXP-5`,
    title: `Side-by-side diff view`,
    status: `done`,
    priority: `medium`,
    assignee: DS,
  },
  {
    id: `EXP-7`,
    title: `Terminal exit-code badges`,
    status: `done`,
    priority: `low`,
  },
]

export const getIssue = (id: string): Issue =>
  ISSUES.find((i) => i.id === id) ?? ISSUES[0]

export const GROUP_ORDER: { status: IssueStatus; label: string }[] = [
  { status: `in_progress`, label: `In Progress` },
  { status: `todo`, label: `Todo` },
  { status: `backlog`, label: `Backlog` },
  { status: `done`, label: `Done` },
]

export const FILTER_STATUSES: Record<FilterTab, IssueStatus[]> = {
  all: [`in_progress`, `todo`, `backlog`, `done`],
  active: [`in_progress`, `todo`],
  backlog: [`backlog`],
}

export const STATUS_LABEL: Record<IssueStatus, string> = {
  backlog: `Backlog`,
  todo: `Todo`,
  in_progress: `In Progress`,
  done: `Done`,
}

export const PRIORITY_LABEL: Record<IssuePriority, string> = {
  none: `No priority`,
  urgent: `Urgent`,
  high: `High`,
  medium: `Medium`,
  low: `Low`,
}

/* ─── Issue detail bodies (rendered GFM, statically) ─── */

export type Inline = { t: string; code?: boolean }

export const ISSUE_BODY: Record<string, Inline[][]> = {
  [`EXP-8`]: [
    [
      {
        t: `When the steer relay drops a WebSocket mid-session, the terminal view goes stale and never recovers. Reconnect with exponential backoff and resume the `,
      },
      { t: `pty`, code: true },
      { t: ` stream from the last acked offset.` },
    ],
    [
      { t: `Repro: kill the relay while a ` },
      { t: `claude`, code: true },
      { t: ` session is streaming — the xterm freezes until a full page reload.` },
    ],
  ],
}

export type ActivityItem =
  | { kind: `comment`; author: string; initials: string; time: string; body: string }
  | { kind: `event`; text: string; time: string }

export const ISSUE_ACTIVITY: Record<string, ActivityItem[]> = {
  [`EXP-8`]: [
    {
      kind: `comment`,
      author: `Danny Strähhuber`,
      initials: `DS`,
      time: `3 hours ago`,
      body: `Backoff should cap at 15s — the relay load balancer kills idle sockets after 60s anyway.`,
    },
    {
      kind: `event`,
      text: `Danny Strähhuber changed status to In Progress`,
      time: `2 hours ago`,
    },
  ],
}

/* ─── Git fixtures ─── */

export type Branch = { name: string; current?: boolean; worktree?: boolean }

export const BRANCHES: Branch[] = [
  { name: `master`, current: true },
  { name: `exp/EXP-8`, worktree: true },
]

export type Commit = { subject: string; meta: string }

export const COMMITS: Commit[] = [
  { subject: `feat(desktop): JetBrains-style IDE shell`, meta: `niach · vor 3 Stunden` },
  { subject: `fix(ios): show compose button only inside a project`, meta: `niach · vor 5 Stunden` },
  { subject: `fix(mobile): Android issue-open crash`, meta: `niach · vor 9 Stunden` },
  { subject: `feat!: masterplan v5 — per-seat billing`, meta: `niach · vor 11 Stunden` },
]

export type GitLetter = `M` | `A` | `D` | `R`
export type Change = { path: string; status: GitLetter }

export const CHANGES: Change[] = [
  { path: `apps/web/src/components/steer-terminal.tsx`, status: `M` },
]

/* ─── Diff fixture: steer-terminal.tsx +24 −6, one hunk ─── */

export type DiffCell = { n: number; text: string; kind: `ctx` | `add` | `del` } | null
export type DiffRow = { l: DiffCell; r: DiffCell }

export const DIFF_FILE = {
  path: `apps/web/src/components/steer-terminal.tsx`,
  add: 24,
  del: 6,
}

export const DIFF_HUNK = `@@ -42,10 +42,28 @@ export function SteerTerminal({ sessionId }: SteerTerminalProps)`

const OLD_REMOVED = [
  `  useEffect(() => {`,
  `    const ws = new WebSocket(steerUrl(sessionId))`,
  `    ws.onmessage = (e) => term.write(decode(e.data))`,
  `    socket.current = ws`,
  `    return () => ws.close()`,
  `  }, [sessionId])`,
]

const NEW_ADDED = [
  `  const retries = useRef(0)`,
  `  const reconnectTimer = useRef<ReturnType<typeof setTimeout>>()`,
  ``,
  `  const connect = useCallback(() => {`,
  `    const ws = new WebSocket(steerUrl(sessionId))`,
  `    ws.onmessage = (e) => term.write(decode(e.data))`,
  `    ws.onopen = () => {`,
  `      retries.current = 0`,
  `    }`,
  `    ws.onclose = () => {`,
  `      const delay = Math.min(1_000 * 2 ** retries.current, 15_000)`,
  `      retries.current += 1`,
  `      reconnectTimer.current = setTimeout(connect, delay)`,
  `    }`,
  `    socket.current = ws`,
  `  }, [sessionId])`,
  ``,
  `  useEffect(() => {`,
  `    connect()`,
  `    return () => {`,
  `      clearTimeout(reconnectTimer.current)`,
  `      socket.current?.close()`,
  `    }`,
  `  }, [connect])`,
]

const buildDiffRows = (): DiffRow[] => {
  const rows: DiffRow[] = [
    {
      l: { n: 42, text: `  const socket = useRef<WebSocket | null>(null)`, kind: `ctx` },
      r: { n: 42, text: `  const socket = useRef<WebSocket | null>(null)`, kind: `ctx` },
    },
    { l: { n: 43, text: ``, kind: `ctx` }, r: { n: 43, text: ``, kind: `ctx` } },
  ]
  NEW_ADDED.forEach((text, i) => {
    rows.push({
      l: i < OLD_REMOVED.length ? { n: 44 + i, text: OLD_REMOVED[i], kind: `del` } : null,
      r: { n: 44 + i, text, kind: `add` },
    })
  })
  rows.push({ l: { n: 50, text: ``, kind: `ctx` }, r: { n: 68, text: ``, kind: `ctx` } })
  rows.push({
    l: { n: 51, text: `  return <TerminalView ref={mount} onData={handleInput} />`, kind: `ctx` },
    r: { n: 69, text: `  return <TerminalView ref={mount} onData={handleInput} />`, kind: `ctx` },
  })
  return rows
}

export const DIFF_ROWS: DiffRow[] = buildDiffRows()

/* ─── File tree ─── */

export type FileNode = {
  name: string
  path: string
  children?: FileNode[]
  git?: GitLetter
  dim?: boolean
}

type RawNode = {
  name: string
  children?: RawNode[]
  git?: GitLetter
  dim?: boolean
}

const d = (name: string, children: RawNode[]): RawNode => ({ name, children })
const f = (name: string, extra?: { git?: GitLetter; dim?: boolean }): RawNode => ({
  name,
  ...extra,
})

const attachPaths = (nodes: RawNode[], base: string): FileNode[] =>
  nodes.map((n) => {
    const path = base ? `${base}/${n.name}` : n.name
    return {
      name: n.name,
      path,
      git: n.git,
      dim: n.dim,
      children: n.children ? attachPaths(n.children, path) : undefined,
    }
  })

export const FILE_TREE: FileNode[] = attachPaths(
  [
    d(`.github`, [d(`workflows`, [f(`build-desktop.yml`), f(`build-issues-web.yml`)])]),
    d(`apps`, [
      d(`android`, [f(`build.gradle.kts`), f(`settings.gradle.kts`)]),
      d(`desktop`, [d(`crates`, [f(`Cargo.toml`)]), f(`Cargo.toml`)]),
      d(`ios`, [f(`Project.swift`), f(`Tuist.swift`)]),
      d(`web`, [
        d(`src`, [
          d(`components`, [f(`issue-list.tsx`), f(`steer-terminal.tsx`, { git: `M` })]),
          d(`routes`, [f(`index.tsx`)]),
          f(`styles.css`),
        ]),
        f(`package.json`),
        f(`vite.config.ts`),
      ]),
    ]),
    d(`packages`, [
      d(`db-schema`, [f(`package.json`)]),
      d(`design-tokens`, [f(`tokens.json`), f(`package.json`)]),
      d(`widget`, [f(`package.json`)]),
    ]),
    f(`Caddyfile`, { dim: true }),
    f(`docker-compose.yaml`),
    f(`package.json`),
    f(`README.md`),
  ],
  ``,
)

/* ─── Read-only code tab: root package.json ─── */

export const PACKAGE_JSON = `{
  "name": "exponential",
  "private": true,
  "workspaces": ["apps/*", "packages/*"],
  "scripts": {
    "dev": "bun run --filter @exp/web dev",
    "build": "bun run build:widget && bun run build:web",
    "typecheck": "bun run --filter @exp/web typecheck",
    "migrate": "cd apps/web && drizzle-kit migrate",
    "test": "bun run --filter @exp/web test"
  },
  "packageManager": "bun@1.2.19"
}`

/* ─── "Start coding" scripted claude session (~8 lines) ─── */

export type ScriptLineKind = `ok` | `cmd` | `claude`
export type ScriptLine = { kind: ScriptLineKind; text: string }

export const CODING_SCRIPT: ScriptLine[] = [
  { kind: `ok`, text: `Created worktree .worktrees/EXP-8 on branch exp/EXP-8` },
  { kind: `cmd`, text: `claude --dangerously-skip-permissions` },
  { kind: `claude`, text: `Reading issue EXP-8 — Live-steer terminal reconnect` },
  { kind: `claude`, text: `Plan: reconnect with exponential backoff, resume stream` },
  { kind: `claude`, text: `Edited apps/web/src/components/steer-terminal.tsx (+24 -6)` },
  { kind: `cmd`, text: `git push -u origin exp/EXP-8` },
  { kind: `claude`, text: `Opened PR #214 — Live-steer terminal reconnect` },
  { kind: `ok`, text: `Session finished · 1 file changed` },
]

export const SHELL_TAB_TITLE = `~/E/r/N/exponential`
export const CLAUDE_TAB_TITLE = `claude · EXP-8`
