import { DevicesView, IssueListView, type DeviceRow, type IssueRow } from "@exp/mcp-apps"

import type { StyleguideEntry } from "./types.ts"

// EXP-1183: the MCP Apps views (`packages/mcp-apps`) — what an MCP Apps host
// (OpenClaw's dashboard, Claude, ChatGPT) mounts beside an
// `exponential_issues_show` call. The REAL `IssueListView`: the web board's
// group band over flat hairline rows, built from `@exp/ui` alone. EXP-1199:
// the REAL `DevicesView` beside it, an own online machine's logins with the
// "Sign in" / "Add account" controls (`exponential_devices_account_login`).

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

const DEVICES = [
  {
    deviceId: `studio`,
    label: `Studio`,
    kind: `desktop`,
    platform: `macos`,
    version: `0.14.60`,
    online: true,
    isDefault: true,
    caps: [`agent-login`],
    agents: [`claude`],
    unauthedAgents: [`codex`],
    agentAccounts: {
      claude: {
        signedIn: true,
        profiles: [
          { id: `9f8e7d6c`, signedIn: true, active: true, email: `dana@acme.test`, plan: `Max` },
          { id: `a1b2c3d4`, signedIn: true, health: `needs_relogin`, email: `ops@acme.test` },
        ],
      },
      codex: { signedIn: false },
    },
  },
] as unknown as DeviceRow[]

export const entry: StyleguideEntry = {
  id: `mcp-app-views`,
  section: `special`,
  owner: `EXP-1183`,
  title: `MCP App views`,
  blurb: `Exponential inside an MCP Apps host. \`exponential_issues_show\` mounts the issue list (status group bands over the board's flat rows; a row opens the issue in place: pills, GFM description, comments), \`exponential_sessions_list\` the run list (Running / In review / Ended bands; a row opens the run), \`exponential_sessions_get\` the run's report as the Results Guide, \`exponential_notifications_list\` the inbox (a row opens its issue), \`exponential_devices_list\` the Devices page (an own online machine offers Sign in on a dead login and Add account, both through \`exponential_devices_account_login\`). One self-contained document served as \`ui://exponential/*\`, themed by the host's light/dark.`,
  status: {
    web: {
      state: `ok`,
      symbol: `IssueListView / IssueDetailView / RunsListView / RunView / InboxView / DevicesView`,
      file: `packages/mcp-apps/src/issue-list-view.tsx`,
      note: `served by apps/web/src/lib/mcp/apps.ts`,
    },
    desktop: { state: `n/a`, note: `MCP Apps render inside the third-party host, never in a client.` },
    ios: { state: `n/a`, note: `MCP Apps render inside the third-party host, never in a client.` },
    android: { state: `n/a`, note: `MCP Apps render inside the third-party host, never in a client.` },
  },
  island: () => (
    <div className="flex flex-col gap-4">
      <IssueListView issues={ISSUES} />
      <DevicesView devices={DEVICES} />
    </div>
  ),
}
