/* ─── IDE playground fixture data — dogfood: Exponential building Exponential ─── */

export type IssueStatus = `backlog` | `in_progress` | `in_review` | `done`
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

export const DEMO_BOARD = { name: `Exponential`, color: `#a1a1aa` }

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
    status: `in_review`,
    priority: `medium`,
    due: `Jul 15`,
  },
  {
    id: `EXP-12`,
    title: `Attachment paste uploads`,
    status: `backlog`,
    priority: `none`,
  },
  {
    id: `EXP-9`,
    title: `Issue board keyboard polish`,
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
  { status: `in_review`, label: `In Review` },
  { status: `backlog`, label: `Backlog` },
  { status: `done`, label: `Done` },
]

export const FILTER_STATUSES: Record<FilterTab, IssueStatus[]> = {
  all: [`in_progress`, `in_review`, `backlog`, `done`],
  active: [`in_progress`, `in_review`],
  backlog: [`backlog`],
}

export const STATUS_LABEL: Record<IssueStatus, string> = {
  backlog: `Backlog`,
  in_progress: `In Progress`,
  in_review: `In Review`,
  done: `Done`,
}

export const PRIORITY_LABEL: Record<IssuePriority, string> = {
  none: `No priority`,
  urgent: `Urgent`,
  high: `High`,
  medium: `Medium`,
  low: `Low`,
}

/* ─── Inbox fixtures — single Linear-style activity stream ─── */

export type InboxType =
  | `issue_assigned`
  | `issue_comment`
  | `issue_status_changed`
  | `pr_opened`
  | `pr_merged`

export type InboxItem = {
  id: string
  type: InboxType
  issueId: string
  title: string
  sentence: string
  time: string
  unread: boolean
}

export const INBOX_ITEMS: InboxItem[] = [
  {
    id: `n1`,
    type: `pr_opened`,
    issueId: `EXP-8`,
    title: `Live-steer terminal reconnect`,
    sentence: `Claude opened pull request #214 for EXP-8`,
    time: `2m`,
    unread: true,
  },
  {
    id: `n2`,
    type: `issue_comment`,
    issueId: `EXP-12`,
    title: `Attachment paste uploads`,
    sentence: `Danny commented: paste should reuse the drag-drop path`,
    time: `26m`,
    unread: true,
  },
  {
    id: `n3`,
    type: `issue_assigned`,
    issueId: `EXP-11`,
    title: `Issue board keyboard navigation`,
    sentence: `Danny assigned you EXP-11`,
    time: `1h`,
    unread: true,
  },
  {
    id: `n4`,
    type: `pr_merged`,
    issueId: `EXP-5`,
    title: `Side-by-side diff view`,
    sentence: `Danny merged the pull request for EXP-5`,
    time: `3h`,
    unread: false,
  },
  {
    id: `n5`,
    type: `issue_status_changed`,
    issueId: `EXP-7`,
    title: `Terminal exit-code badges`,
    sentence: `Danny changed the status to Done`,
    time: `5h`,
    unread: false,
  },
]

/* ─── My Issues — subset of ISSUES assigned to the signed-in user ─── */

export const MY_ISSUE_IDS: string[] = [`EXP-8`, `EXP-5`]

/* ─── Reviews — issues with an open PR (one issue = one PR = one branch) ─── */

export type Review = {
  issueId: string
  identifier: string
  title: string
  branch: string
  prNumber: number
}

export const REVIEWS: Review[] = [
  {
    issueId: `EXP-8`,
    identifier: `EXP-8`,
    title: `Live-steer terminal reconnect`,
    branch: `exp/EXP-8`,
    prNumber: 214,
  },
  {
    issueId: `EXP-11`,
    identifier: `EXP-11`,
    title: `Issue board keyboard navigation`,
    branch: `exp/EXP-11`,
    prNumber: 209,
  },
]

/* ─── Issue detail bodies (rendered GFM, statically) ───
   `ref` renders a same-team #issue pill, `mention` an @member pill;
   both are plain text in the markdown source, pills only at render time. */

export type Inline = { t: string; code?: boolean; ref?: boolean; mention?: boolean }

export const ISSUE_BODY: Record<string, Inline[][]> = {
  [`EXP-8`]: [
    [
      {
        t: `When the steer relay drops a WebSocket mid-session, the activity feed goes stale and never recovers. Reconnect with exponential backoff and resume the `,
      },
      { t: `activity`, code: true },
      { t: ` stream from the last acked seq.` },
    ],
    [
      {
        t: `Repro: kill the relay while a Claude session is streaming. The viewer freezes until a full page reload.`,
      },
    ],
    [
      { t: `Same reconnect contract as ` },
      { t: `EXP-5`, ref: true },
      { t: `. Ping ` },
      { t: `Danny Strähhuber`, mention: true },
      { t: ` once the relay patch lands.` },
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
      body: `Backoff should cap at 15s. The relay load balancer kills idle sockets after 60s anyway.`,
    },
    {
      kind: `event`,
      text: `Danny Strähhuber changed status to In Progress`,
      time: `2 hours ago`,
    },
  ],
}

/* ─── Git fixtures ─── */

export type Commit = { subject: string; meta: string }

export const COMMITS: Commit[] = [
  { subject: `feat(desktop): JetBrains-style IDE shell`, meta: `niach · 3 hours ago` },
  { subject: `fix(ios): show compose button only inside a board`, meta: `niach · 5 hours ago` },
  { subject: `fix(mobile): Android issue-open crash`, meta: `niach · 9 hours ago` },
  { subject: `feat!: masterplan v5 — per-seat billing`, meta: `niach · 11 hours ago` },
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

/* EXP-895: the ONE diff view is UNIFIED — `old gutter · new gutter · sign ·
   text` rows. The same hunk, as the desktop's per-file card lists it. */
export type UnifiedRow = { old: number | null; new: number | null; kind: `ctx` | `add` | `del`; text: string }

const buildUnifiedRows = (): UnifiedRow[] => {
  const rows: UnifiedRow[] = [
    { old: 42, new: 42, kind: `ctx`, text: `  const socket = useRef<WebSocket | null>(null)` },
    { old: 43, new: 43, kind: `ctx`, text: `` },
  ]
  OLD_REMOVED.forEach((text, i) => rows.push({ old: 44 + i, new: null, kind: `del`, text }))
  NEW_ADDED.forEach((text, i) => rows.push({ old: null, new: 44 + i, kind: `add`, text }))
  rows.push({ old: 50, new: 68, kind: `ctx`, text: `` })
  rows.push({ old: 51, new: 69, kind: `ctx`, text: `  return <TerminalView ref={mount} onData={handleInput} />` })
  return rows
}

export const DIFF_UNIFIED: UnifiedRow[] = buildUnifiedRows()

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

/* ─── The coding session a Start launches — the TRANSCRIPT the thread's
   Conversation shows while the thread has ONE session (NEW-6/NEW-46): the
   agent's prose beside its sparkle mark, and THE Trace lines between
   (fixtures/trace.json: `Thought for 5s`, `Read <path>`, `Edited <path>`,
   `Ran <command>`, `Failed: 1 of 12 tests in <suite>`). The turn ends on a
   question, which floats over the composer as a Dock card (dock.json). ─── */

export type TraceIcon = `reasoning` | `read` | `edit` | `terminal` | `subagent` | `failed`

export type FeedRow =
  /* Agent prose (14/22, the transcript's body type). */
  | { kind: `narration`; text: string }
  /* ONE Trace line (sm geometry); `chevron` = a collapsible line. */
  | {
      kind: `trace`
      icon: TraceIcon
      title: string
      description?: string
      chevron?: boolean
      error?: boolean
    }
  /* A person's message: the right-aligned bubble over `author · HH:MM`. */
  | { kind: `user`; text: string }

/* The agent's question: a Dock card while it waits, its sheet on a click. */
export type Question = {
  text: string
  options: { label: string; description: string }[]
}

export const codingScriptFor = (issue: Issue): FeedRow[] => [
  { kind: `trace`, icon: `reasoning`, title: `Thought for 4s` },
  { kind: `trace`, icon: `read`, title: `Read ${DIFF_FILE.path}` },
  {
    kind: `narration`,
    text: `${issue.title}: the socket is opened once and never reopened. I'll reconnect with exponential backoff and resume from the last event the viewer acked.`,
  },
  { kind: `trace`, icon: `edit`, title: `Edited ${DIFF_FILE.path}` },
  { kind: `trace`, icon: `terminal`, title: `Ran bun test steer`, chevron: true },
  {
    kind: `trace`,
    icon: `failed`,
    title: `Failed: 1 of 12 tests in steer-terminal`,
    chevron: true,
    error: true,
  },
  { kind: `trace`, icon: `edit`, title: `Edited ${DIFF_FILE.path}` },
  { kind: `trace`, icon: `terminal`, title: `Ran bun test steer`, chevron: true },
  {
    kind: `narration`,
    text: `All 12 steer tests pass. The backoff doubles from 1s and caps at 15s, and a reconnect resumes the stream where it stopped.`,
  },
]

export const questionFor = (): Question => ({
  text: `The backoff caps at 15s. Keep that, or make the cap a team setting?`,
  options: [
    { label: `Keep the 15s cap`, description: `Matches the relay's own backoff` },
    { label: `Make it a team setting`, description: `A settings row and a migration on top` },
  ],
})

/* The machine and login the Agent page's options line names. */
export const RUN_DEVICE = `Danny's MacBook Pro`
export const RUN_ACCOUNT = `danny@exponential.at`
export const RUN_MODEL = `Fable`

/* ─── The rail's team sections (sidebar.rs): the team's AGENTS (each its
   own glyph in its colour), its APPS and
   its boards. ─── */
export type RailAgent = { name: string; color: string; icon: `bug` | `palette` | `package` }

export const RAIL_AGENTS: RailAgent[] = [
  { name: `Triage`, color: `#a855f7`, icon: `bug` },
  { name: `Release`, color: `#22c55e`, icon: `package` },
]
