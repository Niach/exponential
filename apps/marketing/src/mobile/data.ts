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

export const mobDetailIssue = {
  identifier: `EXP-8`,
  title: `Live-steer terminal reconnect`,
  status: `In Progress`,
  priority: `High`,
  assignee: { initials: `DS`, name: `Danny Strähhuber` },
  description: [
    `When the steer relay drops a socket mid-session, the terminal view keeps rendering the last frame instead of reconnecting.`,
    `Reconnect with backoff and replay the scrollback buffer so remote viewers never see a frozen session.`,
  ],
  event: `Danny changed status to In Progress · 2 hours ago`,
  comment: {
    author: `Danny Strähhuber`,
    initials: `DS`,
    time: `1 hour ago`,
    body: `Repro: toggle Wi-Fi while a session is streaming. The cursor freezes but the session keeps running fine.`,
  },
}
