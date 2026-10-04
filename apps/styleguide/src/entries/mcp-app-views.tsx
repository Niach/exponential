import { IssueListView, type IssueRow } from "@exp/mcp-apps"

import type { StyleguideEntry } from "./types.ts"

// EXP-1183: the MCP Apps views (`packages/mcp-apps`) — what an MCP Apps host
// (OpenClaw's dashboard, Claude, ChatGPT) mounts beside an
// `exponential_issues_show` call. The REAL `IssueListView`: the web board's
// group band over flat hairline rows, built from `@exp/ui` alone.

/** The views' own sources, for the island stylesheet's Tailwind scan
 *  (relative to this app's `src/`, the compile `base`). */
export const MCP_APPS_CSS_SOURCE = `../../../packages/mcp-apps/src/**/*.{ts,tsx}`

const row = (
  identifier: string,
  title: string,
  status: IssueRow[`status`],
  priority: IssueRow[`priority`],
  pr?: { number: number; state: string }
): IssueRow => ({
  id: identifier,
  identifier,
  title,
  status,
  priority,
  prNumber: pr?.number ?? null,
  prState: pr?.state ?? null,
  prUrl: pr ? `https://github.com/acme/app/pull/${pr.number}` : null,
})

const ISSUES: IssueRow[] = [
  row(`APP-21`, `Offline queue for issue edits`, `in_progress`, `high`),
  row(`APP-18`, `Reduce cold start below 800 ms`, `in_progress`, `urgent`),
  row(`APP-17`, `Thumbnail pipeline for faster image loading`, `in_review`, `medium`, { number: 56, state: `open` }),
  row(`APP-12`, `Quick-add issue from the home screen widget`, `backlog`, `low`),
  row(`APP-9`, `Fix crash when uploading HEIC photos`, `done`, `urgent`, { number: 41, state: `merged` }),
]

export const entry: StyleguideEntry = {
  id: `mcp-app-views`,
  section: `special`,
  owner: `EXP-1183`,
  title: `MCP App views`,
  blurb: `Exponential inside an MCP Apps host. \`exponential_issues_show\` mounts the issue list (status group bands over the board's flat rows; a row opens the issue in place: pills, GFM description, comments), \`exponential_sessions_get\` the run's report as the Results Guide. One self-contained document served as \`ui://exponential/*\`, themed by the host's light/dark.`,
  status: {
    web: {
      state: `ok`,
      symbol: `IssueListView / IssueDetailView / RunView`,
      file: `packages/mcp-apps/src/issue-list-view.tsx`,
      note: `served by apps/web/src/lib/mcp/apps.ts`,
    },
    desktop: { state: `n/a`, note: `MCP Apps render inside the third-party host, never in a client.` },
    ios: { state: `n/a`, note: `MCP Apps render inside the third-party host, never in a client.` },
    android: { state: `n/a`, note: `MCP Apps render inside the third-party host, never in a client.` },
  },
  island: () => <IssueListView issues={ISSUES} />,
}
