/* ─── Web-app demo fixtures — ONLY what the web chrome adds ───
   Issues, inbox items, reviews and issue bodies come from the single fixture
   universe in ../ide/data (dogfood: Exponential building Exponential). */
import { PROJECT } from "../ide/data"

/* ─── Sidebar boards — colored icons ─── */

export type DemoProjectIcon = `code` | `kanban` | `megaphone`

export type DemoProject = {
  name: string
  slug: string
  color: string
  icon: DemoProjectIcon
}

export const WEB_PROJECTS: DemoProject[] = [
  { name: PROJECT.name, slug: `exponential`, color: PROJECT.color, icon: `code` },
  { name: `Mobile Apps`, slug: `mobile-apps`, color: `#f97316`, icon: `kanban` },
  { name: `Feedback`, slug: `feedback`, color: `#22c55e`, icon: `megaphone` },
]

/* The demo user (matches the assignee fixture DS in ide/data). */
export const WEB_USER = {
  name: `Danny Strähhuber`,
  initials: `DS`,
  email: `danny@exponential.at`,
}

/* Running coding sessions — feeds the Agents nav green dot. */
export const AGENTS_RUNNING = 1

/* ─── Support (helpdesk) threads — server-only tables in the real app,
   so the demo carries its own conversation fixtures. Each thread links an
   issue from the shared fixture universe. ─── */

export type SupportMessage = {
  direction: `inbound` | `outbound`
  /* Outbound only: internal notes are never emailed to the reporter. */
  internal?: boolean
  author: string
  body: string
  time: string
}

export type SupportThread = {
  id: string
  reporterName: string
  reporterEmail: string
  issueId: string
  lastSeen: string
  resolved?: boolean
  unread?: boolean
  time: string
  messages: SupportMessage[]
}

export const SUPPORT_THREADS: SupportThread[] = [
  {
    id: `t-mara`,
    reporterName: `Mara Winkler`,
    reporterEmail: `mara@heliolabs.io`,
    issueId: `EXP-13`,
    lastSeen: `12m ago`,
    unread: true,
    time: `12m`,
    messages: [
      {
        direction: `inbound`,
        author: `Mara Winkler`,
        body: `Hi — when I attach a screenshot to a bug report the upload spinner never finishes. Safari 17 on macOS.`,
        time: `1h`,
      },
      {
        direction: `outbound`,
        author: `Danny Strähhuber`,
        body: `Thanks Mara — reproduced on Safari. The annotation layer is blocking the upload callback; fix is underway.`,
        time: `48m`,
      },
      {
        direction: `outbound`,
        internal: true,
        author: `Danny Strähhuber`,
        body: `Same root cause as EXP-13 — the annotation flatten re-encode stalls on Safari WebP. Fix rides the next widget release.`,
        time: `45m`,
      },
      {
        direction: `inbound`,
        author: `Mara Winkler`,
        body: `Great, thanks for the quick response! Happy to test a build.`,
        time: `12m`,
      },
    ],
  },
  {
    id: `t-jonas`,
    reporterName: `Jonas Petersen`,
    reporterEmail: `jonas@fjordworks.no`,
    issueId: `EXP-12`,
    lastSeen: `3h ago`,
    time: `3h`,
    messages: [
      {
        direction: `inbound`,
        author: `Jonas Petersen`,
        body: `Is there a way to paste images straight from the clipboard into a report?`,
        time: `4h`,
      },
      {
        direction: `outbound`,
        author: `Danny Strähhuber`,
        body: `Not yet — paste uploads are tracked as EXP-12. I'll follow up here the moment it ships.`,
        time: `3h`,
      },
    ],
  },
  {
    id: `t-sofia`,
    reporterName: `Sofia Marino`,
    reporterEmail: `sofia@brightapps.co`,
    issueId: `EXP-5`,
    lastSeen: `2d ago`,
    resolved: true,
    time: `2d`,
    messages: [
      {
        direction: `inbound`,
        author: `Sofia Marino`,
        body: `The side-by-side diff view clips the right pane on ultrawide monitors.`,
        time: `3d`,
      },
      {
        direction: `outbound`,
        author: `Danny Strähhuber`,
        body: `Fixed in last week's release — thanks for the report!`,
        time: `2d`,
      },
    ],
  },
]

export const getThread = (id: string): SupportThread =>
  SUPPORT_THREADS.find((t) => t.id === id) ?? SUPPORT_THREADS[0]
