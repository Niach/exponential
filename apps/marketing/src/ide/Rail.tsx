/* ─── The labelled left rail (EXP-282/285/525) — 164px, glass wash, full
   window height. Order (sidebar.rs, EXP-699/686/706 — the mobile tab-bar
   order): titlebar strip (traffic lights + collapse toggle) · Search ·
   divider · Inbox / Support / Devices / Actions / Automations / Reviews ·
   divider · Boards group · divider · Files / Source Control · Getting
   started · account row. ─── */
import type { ReactNode } from "react"
import { INBOX_ITEMS, PROJECT, REVIEWS } from "./data"
import { useIde } from "./state"
import {
  IcBot,
  IcCode,
  IcFolder,
  IcGitMerge,
  IcGitPullRequest,
  IcInbox,
  IcLifeBuoy,
  IcMegaphone,
  IcMonitor,
  IcPanelLeftClose,
  IcPlus,
  IcKanban,
  IcSearch,
  IcSettings,
  IcSparkles,
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
      <Icon size={10} style={tint ? { color: tint } : undefined} />
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
        <div className="ide-flex1" />
        <span className="ide-rail-toggle">
          <IcPanelLeftClose size={12} />
        </span>
      </div>
      <RailRow Icon={IcSearch} label="Search" />
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
          <IcPlus size={10} />
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
        badge={<IcAlert size={10} className="ide-c-yellow" />}
        onClick={on(openSourceControl)}
      />
      <div className="ide-rail-spacer" />
      <RailRow Icon={IcSparkles} label="Getting started" />
      <div className="ide-rail-account">
        <span className="ide-railrow is-account">
          <span className="ide-avatar ide-avatar-me">DS</span>
          <span className="ide-railrow-label">Danny</span>
        </span>
        <span className="ide-rail-gear">
          <IcSettings size={11} />
        </span>
      </div>
    </div>
  )
}
