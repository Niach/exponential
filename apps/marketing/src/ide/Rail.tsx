/* ─── The rail (sidebar.rs RailView, EXP-723/870) — it NEVER leaves the
   window. Labelled it is the 272px left column: the web-style header (team
   switcher · Search · New issue) · Inbox / Support / Devices / Actions /
   Automations / Reviews / Agent (the live-run COUNT badge) · Boards ·
   "This device": Files / Source Control · the What's new card · Getting
   started · the account row with the new-terminal button and the gear.
   While a list is folded in beside an open issue it is the 48px ICON
   column: the same destinations as 32px squares, labels demoted to
   tooltips, everything that needs a label to mean anything dropped. The
   Sessions section is gone (EXP-870): runs live in top tabs. ─── */
import type { ReactNode } from "react"
import { INBOX_ITEMS, PROJECT, REVIEWS } from "./data"
import { isLive, useIde, type Tool } from "./state"
import {
  IcAlert,
  IcBot,
  IcChevsUpDown,
  IcCode,
  IcFolder,
  IcGitMerge,
  IcGitPullRequest,
  IcInbox,
  IcKanban,
  IcLifeBuoy,
  IcMegaphone,
  IcMessageCircle,
  IcMonitor,
  IcPlus,
  IcSearch,
  IcSettings,
  IcSparkles,
  IcSquarePen,
  IcSquareTerminal,
  IcX,
  IcZap,
  type IdeIcon,
} from "./icons"

/* The rail's board group. The active board is the fixture project; the two
   companions exist so the group reads like the real team sidebar. */
export type RailBoard = { name: string; color: string; Icon: IdeIcon }

export const BOARDS: RailBoard[] = [
  { name: PROJECT.name, color: `#6366f1`, Icon: IcCode },
  /* Same companions as webui/data.ts WEB_PROJECTS — one team across the demos. */
  { name: `Mobile Apps`, color: `#f97316`, Icon: IcKanban },
  { name: `Feedback`, color: `#22c55e`, Icon: IcMegaphone },
]

export const ACTIVE_BOARD = BOARDS[0]

/* The team the demo signs in as — the switcher names it. */
const TEAM_NAME = `Exponential`

/* crate::changelog::LATEST.summary — the What's new card's one-line teaser. */
const WHATS_NEW_SUMMARY = `An issue and its coding run now share one header, and the diff opens full page.`

type Entry = {
  key: string
  Icon: IdeIcon
  label: string
  active?: boolean
  tint?: string
  badge?: ReactNode
  onClick?: () => void
}

function RailRow({ Icon, label, active, tint, badge, onClick }: Omit<Entry, `key`>) {
  return (
    <button
      className={`ide-railrow${active ? ` is-active` : ``}${onClick ? ` is-click` : ``}`}
      type="button"
      onClick={onClick}
    >
      <Icon size={14} style={tint ? { color: tint } : undefined} />
      <span className="ide-railrow-label">{label}</span>
      {badge}
    </button>
  )
}

/* rail_compact_button: a 32px square, the label as a tooltip, the badge on
   the top-right corner. */
function RailSquare({ Icon, label, active, tint, badge, onClick }: Omit<Entry, `key`>) {
  return (
    <button
      className={`ide-railsq${active ? ` is-active` : ``}${onClick ? ` is-click` : ``}`}
      type="button"
      title={label}
      aria-label={label}
      onClick={onClick}
    >
      <Icon size={14} style={tint ? { color: tint } : undefined} />
      {badge && <span className="ide-railsq-badge">{badge}</span>}
    </button>
  )
}

export function Rail({ compact = false }: { compact?: boolean }) {
  const { tool, setTool, active, interactive, goneReviews, inboxRead, runs, openComposer } =
    useIde()
  const go = (next: Tool) => (interactive ? () => setTool(next) : undefined)
  /* An entry reads selected while ITS screen is the main view (EXP-851). */
  const on = (t: Tool) => tool === t && active === null
  const openReviews = REVIEWS.filter((r) => !goneReviews.has(r.issueId)).length
  const unreadInbox = INBOX_ITEMS.some((n) => n.unread && !inboxRead.has(n.id))
  const liveRuns = runs.filter((r) => isLive(r.state)).length

  const nav: Entry[] = [
    {
      key: `inbox`,
      Icon: IcInbox,
      label: `Inbox`,
      active: on(`inbox`),
      /* `inbox_badge` is a PRIMARY dot, not the review green. */
      badge: unreadInbox ? <span className="ide-rail-dot is-primary" /> : undefined,
      onClick: go(`inbox`),
    },
    { key: `support`, Icon: IcLifeBuoy, label: `Support` },
    { key: `devices`, Icon: IcMonitor, label: `Devices` },
    { key: `actions`, Icon: IcBot, label: `Actions` },
    { key: `automations`, Icon: IcZap, label: `Automations` },
    {
      key: `reviews`,
      Icon: IcGitPullRequest,
      label: `Reviews`,
      active: on(`reviews`),
      badge: openReviews > 0 ? <span className="ide-rail-dot" /> : undefined,
      onClick: go(`reviews`),
    },
    {
      /* EXP-825: the Agent page's composer IS the launcher; EXP-870: the
         badge counts my live runs. */
      key: `agent`,
      Icon: IcMessageCircle,
      label: `Agent`,
      active: on(`agent`),
      badge: liveRuns > 0 ? <span className="ide-rail-count">{liveRuns}</span> : undefined,
      onClick: interactive ? () => openComposer([]) : undefined,
    },
  ]
  const boards: Entry[] = BOARDS.map((board, i) => ({
    key: board.name,
    Icon: board.Icon,
    label: board.name,
    tint: board.color,
    active: i === 0 && on(`issues`),
    onClick: i === 0 ? go(`issues`) : undefined,
  }))
  const device: Entry[] = [
    { key: `files`, Icon: IcFolder, label: `Files`, active: on(`files`), onClick: go(`files`) },
    {
      key: `sc`,
      Icon: IcGitMerge,
      label: `Source Control`,
      active: on(`source-control`),
      badge: <IcAlert size={11} className="ide-c-yellow" />,
      onClick: go(`source-control`),
    },
  ]

  if (compact) {
    return (
      <div className="ide-rail is-compact">
        <span className="ide-rail-teamavatar" title={TEAM_NAME}>
          {TEAM_NAME[0]}
        </span>
        <span className="ide-rail-headbtn" title="Search">
          <IcSearch size={14} />
        </span>
        <span className="ide-rail-headbtn is-primary" title="New issue">
          <IcSquarePen size={14} />
        </span>
        <div className="ide-rail-div" />
        {nav.map(({ key, ...e }) => (
          <RailSquare key={key} {...e} />
        ))}
        <div className="ide-rail-div" />
        {boards.map(({ key, ...e }) => (
          <RailSquare key={key} {...e} />
        ))}
        <div className="ide-rail-div" />
        {device.map(({ key, ...e }) => (
          <RailSquare key={key} {...e} />
        ))}
        <div className="ide-rail-spacer" />
        <span className="ide-rail-gear" title="New terminal">
          <IcSquareTerminal size={14} />
        </span>
        <span className="ide-rail-gear">
          <IcSettings size={14} />
        </span>
        <span className="ide-avatar ide-avatar-me">DS</span>
      </div>
    )
  }

  return (
    <div className="ide-rail">
      {/* sidebar.rs render_header (EXP-723): the switcher names the ACTIVE
          team; Search and the PRIMARY New issue button beside it. */}
      <div className="ide-rail-head">
        <button className="ide-rail-team" type="button">
          <span className="ide-rail-teamavatar">{TEAM_NAME[0]}</span>
          <span className="ide-rail-teamname">{TEAM_NAME}</span>
          <IcChevsUpDown size={11} className="ide-c-muted" />
        </button>
        <span className="ide-rail-headbtn" title="Search">
          <IcSearch size={14} />
        </span>
        <span className="ide-rail-headbtn is-primary" title="New issue">
          <IcSquarePen size={14} />
        </span>
      </div>
      <div className="ide-rail-div" />
      {nav.map(({ key, ...e }) => (
        <RailRow key={key} {...e} />
      ))}
      <div className="ide-rail-div" />
      <div className="ide-rail-grouphead">
        <span>Boards</span>
        <span className="ide-rail-plus">
          <IcPlus size={11} />
        </span>
      </div>
      {boards.map(({ key, ...e }) => (
        <RailRow key={key} {...e} />
      ))}
      <div className="ide-rail-div" />
      <div className="ide-rail-sectionlabel">This device</div>
      {device.map(({ key, ...e }) => (
        <RailRow key={key} {...e} />
      ))}
      <div className="ide-rail-spacer" />
      {/* sidebar.rs render_whats_new_card (EXP-723). */}
      <div className="ide-whatsnew">
        <div className="ide-whatsnew-row">
          <IcMegaphone size={12.25} />
          <span className="ide-whatsnew-title">What&apos;s new</span>
          <span className="ide-whatsnew-x" title="Dismiss">
            <IcX size={10.5} />
          </span>
        </div>
        <div className="ide-whatsnew-sum">{WHATS_NEW_SUMMARY}</div>
      </div>
      <RailRow Icon={IcSparkles} label="Getting started" />
      <div className="ide-rail-account">
        <span className="ide-railrow is-account">
          <span className="ide-avatar ide-avatar-me">DS</span>
          <span className="ide-railrow-label">Danny</span>
          <IcChevsUpDown size={11} />
        </span>
        <span className="ide-rail-gear" title="New terminal">
          <IcSquareTerminal size={12.25} />
        </span>
        <span className="ide-rail-gear">
          <IcSettings size={12.25} />
        </span>
      </div>
    </div>
  )
}
