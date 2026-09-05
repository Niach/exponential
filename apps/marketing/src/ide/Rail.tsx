/* ─── The labelled left rail (EXP-282/285/525/723) — 208px (sidebar.rs
   RAIL_W), glass wash, full window height, ALWAYS open since EXP-723. Order
   (sidebar.rs): titlebar strip (traffic lights only — the collapse toggle is
   gone) · the web-style header (team switcher · Search · New issue) ·
   divider · Inbox / Support / Devices / Actions / Automations / Reviews ·
   divider · Boards group · divider · Files / Source Control · spacer · the
   What's new card · Getting started · the account row + settings gear. ─── */
import type { ReactNode } from "react"
import { INBOX_ITEMS, PROJECT, REVIEWS } from "./data"
import { useIde } from "./state"
import {
  IcBot,
  IcChevsUpDown,
  IcCode,
  IcFolder,
  IcGitMerge,
  IcGitPullRequest,
  IcInbox,
  IcLifeBuoy,
  IcMegaphone,
  IcMonitor,
  IcPlus,
  IcKanban,
  IcSearch,
  IcSettings,
  IcSparkles,
  IcSquarePen,
  IcX,
  IcZap,
  IcAlert,
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

/* The team the demo signs in as — the switcher names it; its menu lists the
   teams plus New team / Join team. */
const TEAM_NAME = `Exponential`

/* crate::changelog::LATEST.summary — the What's new card's one-line teaser
   is the head changelog entry's summary (apps/web/src/lib/changelog.ts). */
const WHATS_NEW_SUMMARY = `Reply under a comment on web, desktop, iOS and Android, and see when an agent posted a comment over MCP.`

function RailRow({
  Icon,
  label,
  active,
  tint,
  badge,
  onClick,
}: {
  Icon: IdeIcon
  label: string
  active?: boolean
  tint?: string
  badge?: ReactNode
  onClick?: () => void
}) {
  return (
    <button
      className={`ide-railrow${active ? ` is-active` : ``}${onClick ? ` is-click` : ``}`}
      type="button"
      onClick={onClick}
    >
      <Icon size={10.5} style={tint ? { color: tint } : undefined} />
      <span className="ide-railrow-label">{label}</span>
      {badge}
    </button>
  )
}

export function Rail() {
  const { tool, setTool, openSourceControl, interactive, goneReviews, inboxRead } =
    useIde()
  const on = (fn: () => void) => (interactive ? fn : undefined)
  const openReviews = REVIEWS.filter((r) => !goneReviews.has(r.issueId)).length
  const unreadInbox = INBOX_ITEMS.some((n) => n.unread && !inboxRead.has(n.id))
  return (
    <div className="ide-rail">
      <div className="ide-rail-strip">
        <span className="ide-lights">
          <i style={{ background: `#ff5f57` }} />
          <i style={{ background: `#febc2e` }} />
          <i style={{ background: `#28c840` }} />
        </span>
      </div>
      {/* sidebar.rs render_header (EXP-723): the switcher names the ACTIVE
          team; Search and the PRIMARY New issue button call their openers
          directly. */}
      <div className="ide-rail-head">
        <button className="ide-rail-team" type="button">
          <span className="ide-rail-teamavatar">{TEAM_NAME[0]}</span>
          <span className="ide-rail-teamname">{TEAM_NAME}</span>
          <IcChevsUpDown size={10.5} className="ide-c-muted" />
        </button>
        <span className="ide-rail-headbtn" title="Search">
          <IcSearch size={14} />
        </span>
        <span className="ide-rail-headbtn is-primary" title="New issue">
          <IcSquarePen size={14} />
        </span>
      </div>
      <div className="ide-rail-div" />
      <RailRow
        Icon={IcInbox}
        label="Inbox"
        active={tool === `inbox`}
        /* `inbox_badge` is a PRIMARY dot, not the review green. */
        badge={unreadInbox ? <span className="ide-rail-dot is-primary" /> : undefined}
        onClick={on(() => setTool(`inbox`))}
      />
      <RailRow Icon={IcLifeBuoy} label="Support" />
      {/* EXP-686: Devices · Actions · Automations, the three surfaces the
          old Agents entry bundled. The dot is `agents.running`. */}
      <RailRow Icon={IcMonitor} label="Devices" badge={<span className="ide-rail-dot" />} />
      <RailRow Icon={IcBot} label="Actions" />
      <RailRow Icon={IcZap} label="Automations" />
      <RailRow
        Icon={IcGitPullRequest}
        label="Reviews"
        active={tool === `reviews`}
        badge={openReviews > 0 ? <span className="ide-rail-dot" /> : undefined}
        onClick={on(() => setTool(`reviews`))}
      />
      <div className="ide-rail-div" />
      <div className="ide-rail-grouphead">
        <span>Boards</span>
        <span className="ide-rail-plus">
          <IcPlus size={10.5} />
        </span>
      </div>
      {BOARDS.map((board, i) => (
        <RailRow
          key={board.name}
          Icon={board.Icon}
          label={board.name}
          tint={board.color}
          active={i === 0 && tool === `issues`}
          onClick={i === 0 ? on(() => setTool(`issues`)) : undefined}
        />
      ))}
      <div className="ide-rail-div" />
      <RailRow
        Icon={IcFolder}
        label="Files"
        active={tool === `files`}
        onClick={on(() => setTool(`files`))}
      />
      <RailRow
        Icon={IcGitMerge}
        label="Source Control"
        active={tool === `source-control`}
        badge={<IcAlert size={10.5} className="ide-c-yellow" />}
        onClick={on(openSourceControl)}
      />
      <div className="ide-rail-spacer" />
      {/* sidebar.rs render_whats_new_card (EXP-723): shows until the head
          changelog entry is seen; ✕ marks it seen, the card opens the dialog. */}
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
          <IcChevsUpDown size={10.5} />
        </span>
        <span className="ide-rail-gear">
          <IcSettings size={12.25} />
        </span>
      </div>
    </div>
  )
}
