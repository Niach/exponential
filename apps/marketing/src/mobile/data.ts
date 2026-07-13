export type MobPriority = `none` | `high` | `medium`

export type MobStatus = `in_progress` | `todo` | `backlog` | `done`

export type MobIssue = {
  identifier: string
  title: string
  priority: MobPriority
  status: MobStatus
  assignee?: string
  label?: { name: string; color: string }
}

export type MobGroup = {
  status: MobStatus
  label: string
  issues: MobIssue[]
}

export const mobGroups: MobGroup[] = [
  {
    status: `in_progress`,
    label: `In Progress`,
    issues: [
      {
        identifier: `EXP-8`,
        title: `Live-steer terminal reconnect`,
        priority: `high`,
        status: `in_progress`,
        assignee: `DS`,
      },
    ],
  },
  {
    status: `todo`,
    label: `Todo`,
    issues: [
      {
        identifier: `EXP-11`,
        title: `Issue board keyboard navigation`,
        priority: `medium`,
        status: `todo`,
      },
      {
        identifier: `EXP-12`,
        title: `Attachment paste uploads`,
        priority: `none`,
        status: `todo`,
      },
    ],
  },
  {
    status: `backlog`,
    label: `Backlog`,
    issues: [
      {
        identifier: `EXP-9`,
        title: `Recurring issues UI polish`,
        priority: `none`,
        status: `backlog`,
      },
      {
        identifier: `EXP-13`,
        title: `Widget screenshot annotations`,
        priority: `none`,
        status: `backlog`,
        label: { name: `feedback`, color: `#22c55e` },
      },
    ],
  },
  {
    status: `done`,
    label: `Done`,
    issues: [
      {
        identifier: `EXP-5`,
        title: `Side-by-side diff view`,
        priority: `none`,
        status: `done`,
        assignee: `DS`,
      },
      {
        identifier: `EXP-7`,
        title: `Terminal exit-code badges`,
        priority: `none`,
        status: `done`,
      },
    ],
  },
]

/* ─── Second fixture project — cycled via the header project switcher ─── */

export const mobAltGroups: MobGroup[] = [
  {
    status: `in_progress`,
    label: `In Progress`,
    issues: [
      {
        identifier: `MOB-4`,
        title: `Offline issue drafts`,
        priority: `high`,
        status: `in_progress`,
        assignee: `DS`,
      },
    ],
  },
  {
    status: `todo`,
    label: `Todo`,
    issues: [
      {
        identifier: `MOB-7`,
        title: `Push notification deep links`,
        priority: `medium`,
        status: `todo`,
      },
      {
        identifier: `MOB-9`,
        title: `Haptics on swipe actions`,
        priority: `none`,
        status: `todo`,
      },
    ],
  },
  {
    status: `done`,
    label: `Done`,
    issues: [
      {
        identifier: `MOB-2`,
        title: `Widget screenshot viewer`,
        priority: `none`,
        status: `done`,
        assignee: `DS`,
      },
    ],
  },
]

export const mobProjects = [
  { name: `Exponential`, groups: mobGroups },
  { name: `Mobile App`, groups: mobAltGroups },
] as const

/* ─── Search tab fixtures ─── */

export const mobSearchQuery = `auth`

export const mobSearchResults: MobIssue[] = [
  {
    identifier: `EXP-3`,
    title: `Google auth on the register page`,
    priority: `medium`,
    status: `done`,
  },
  {
    identifier: `EXP-14`,
    title: `Auth session refresh on wake`,
    priority: `high`,
    status: `in_progress`,
    assignee: `DS`,
  },
  {
    identifier: `EXP-19`,
    title: `OAuth error toasts`,
    priority: `none`,
    status: `todo`,
  },
]

export const mobAssigned: MobIssue[] = [
  {
    identifier: `EXP-8`,
    title: `Live-steer terminal reconnect`,
    priority: `high`,
    status: `in_progress`,
    assignee: `DS`,
  },
  {
    identifier: `EXP-5`,
    title: `Side-by-side diff view`,
    priority: `none`,
    status: `done`,
    assignee: `DS`,
  },
]

/* ─── Agents tab fixtures — running coding sessions ─── */

export type MobAgent = {
  identifier: string
  title: string
  meta: string
}

export const mobAgents: MobAgent[] = [
  {
    identifier: `EXP-12`,
    title: `Attachment paste uploads`,
    meta: `Claude on dennis-mbp · 12m`,
  },
  {
    identifier: `EXP-8`,
    title: `Live-steer terminal reconnect`,
    meta: `Claude on dennis-mbp · 34m`,
  },
]

/* ─── Live steer screen — chat-style scrubbed activity feed (never raw
   terminal bytes): narration bubbles + compact tool rows, a pinned
   "Latest changes" diff chip, and the "Message the agent…" composer ─── */

export type MobFeedItem =
  | { kind: `narration`; text: string }
  | { kind: `tool`; name: string; detail: string }

export const mobSteerFeed: MobFeedItem[] = [
  { kind: `narration`, text: `Reading EXP-12 — paste should reuse the drag-drop upload path.` },
  { kind: `tool`, name: `Read`, detail: `issue-editor/paste-upload.ts` },
  { kind: `tool`, name: `Edit`, detail: `paste-upload.ts +41 −3` },
  {
    kind: `narration`,
    text: `Paste now goes through the same upload queue as drag-drop, with progress chips.`,
  },
  { kind: `tool`, name: `Bash`, detail: `bun run typecheck · 0 errors` },
  { kind: `narration`, text: `Typecheck is clean — committing and opening the pull request.` },
]

export const mobSteerDiff = { files: 1, add: 41, del: 3 }

/* ─── Releases tab fixtures — mirrors the IDE release fixtures ─── */

export type MobRelease = {
  name: string
  target?: string
  shipped?: string
  done: number
  total: number
  coding?: boolean
}

export const mobReleases: MobRelease[] = [
  { name: `Live steer v2`, target: `Jul 15`, done: 1, total: 4, coding: true },
  { name: `Terminal polish`, shipped: `Jul 2`, done: 1, total: 1 },
]

/* ─── Inbox tab fixtures — single activity stream ─── */

export type MobInboxType = `pr_opened` | `pr_merged` | `comment` | `assigned`

export type MobInboxItem = {
  type: MobInboxType
  identifier: string
  title: string
  sentence: string
  time: string
  unread: boolean
}

export const mobInboxItems: MobInboxItem[] = [
  {
    type: `pr_opened`,
    identifier: `EXP-12`,
    title: `Attachment paste uploads`,
    sentence: `Claude opened pull request #217`,
    time: `2m`,
    unread: true,
  },
  {
    type: `comment`,
    identifier: `EXP-8`,
    title: `Live-steer terminal reconnect`,
    sentence: `Danny commented: backoff should cap at 15s`,
    time: `1h`,
    unread: true,
  },
  {
    type: `pr_merged`,
    identifier: `EXP-5`,
    title: `Side-by-side diff view`,
    sentence: `Danny merged the pull request`,
    time: `3h`,
    unread: false,
  },
  {
    type: `assigned`,
    identifier: `EXP-11`,
    title: `Issue board keyboard navigation`,
    sentence: `Danny assigned you`,
    time: `5h`,
    unread: false,
  },
]

export const mobDetailIssue = {
  identifier: `EXP-8`,
  title: `Live-steer terminal reconnect`,
  status: `In Progress`,
  priority: `High`,
  assignee: { initials: `DS`, name: `Danny Strähhuber` },
  description: [
    `When the steer relay drops a socket mid-session, the activity feed keeps showing the last events instead of reconnecting.`,
    `Reconnect with backoff and replay the missed events so remote viewers never see a frozen session.`,
  ],
  event: `Danny changed status to In Progress · 2 hours ago`,
  comment: {
    author: `Danny Strähhuber`,
    initials: `DS`,
    time: `1 hour ago`,
    body: `Repro: toggle Wi-Fi while a session is streaming. The feed freezes but the session keeps running fine.`,
  },
}
