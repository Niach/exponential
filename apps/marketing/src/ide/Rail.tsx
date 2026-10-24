/* ─── The left column (shell.rs render_left_column + sidebar.rs RailView,
   EXP-723/1192/1294, VAPP-110). It never folds or leaves:
   · the 34px titlebar strip the traffic lights float over;
   · the FIXED header: the team switcher (28px team square, semibold name,
     chevrons-up-down), the ghost Search circle and the PRIMARY New issue
     circle, then the column's 1px rule;
   · the scrolling rail: Agent · Inbox · Threads · Devices (· Drafts while
     any draft is parked), then the Agents, Apps and Boards groups — each a
     muted label with its `+` — and Active (my live threads, hidden while
     none runs);
   · the footer: the muted Getting started row (until its checklist is
     done), then the account button with the Computer menu (Terminal,
     Files, Source Control) and the settings gear on its right. ─── */
import { useState, type ReactNode } from "react"
import { DEMO_BOARD, INBOX_ITEMS, RAIL_AGENTS, getIssue, type RailAgent } from "./data"
import { useIde, type Tool } from "./state"
import { Avatar, RunMark } from "./bits"
import {
  IcAgent,
  IcBug,
  IcChevsUpDown,
  IcCode,
  IcComputer,
  IcCreateIssue,
  IcFiles,
  IcGettingStarted,
  IcInbox,
  IcKanban,
  IcMegaphone,
  IcMonitor,
  IcPackage,
  IcPalette,
  IcPlus,
  IcRocket,
  IcSearch,
  IcSettings,
  IcSourceControl,
  IcTerminal,
  IcThreads,
  type IdeIcon,
} from "./icons"

/* The team's boards, each its board glyph in its colour. The active board
   is the fixture board; the two companions match webui/data.ts. */
export type RailBoard = { name: string; color: string; Icon: IdeIcon }

export const BOARDS: RailBoard[] = [
  { name: DEMO_BOARD.name, color: `#6366f1`, Icon: IcCode },
  { name: `Mobile Apps`, color: `#f97316`, Icon: IcKanban },
  { name: `Feedback`, color: `#22c55e`, Icon: IcMegaphone },
]

export const ACTIVE_BOARD = BOARDS[0]

/* The team the demo signs in as — the switcher names it. */
const TEAM_NAME = `Exponential`

const AGENT_GLYPH: Record<RailAgent[`icon`], IdeIcon> = {
  bug: IcBug,
  palette: IcPalette,
  package: IcPackage,
}

/* `rail_row`: the compact 28px row — 14px glyph, text_sm label, at most a
   badge; the glass ACTIVE fill is the selection. */
function RailRow({
  Icon,
  label,
  tint,
  active,
  badge,
  muted,
  onClick,
}: {
  Icon: IdeIcon
  label: string
  tint?: string
  active?: boolean
  badge?: ReactNode
  muted?: boolean
  onClick?: () => void
}) {
  return (
    <button
      className={`ide-railrow${active ? ` is-active` : ``}${muted ? ` is-muted` : ``}${onClick ? ` is-click` : ``}`}
      type="button"
      onClick={onClick}
    >
      <Icon size={14} style={tint ? { color: tint } : undefined} />
      <span className="ide-railrow-label">{label}</span>
      {badge}
    </button>
  )
}

/* `section_label`: 24px, muted at 50%, text_xs medium, the ghost `+`. */
function SectionLabel({ label, plus = true }: { label: string; plus?: boolean }) {
  return (
    <div className="ide-rail-section">
      <span className="ide-rail-section-label">{label}</span>
      {plus && (
        <span className="ide-rail-plus">
          <IcPlus size={10.5} />
        </span>
      )}
    </div>
  )
}

/* The footer's Computer menu (`rail_computer_entry`): a ghost square that
   opens upward — Terminal, Files, Source Control. */
function ComputerMenu() {
  const { tool, setTool, active, interactive } = useIde()
  const [open, setOpen] = useState(false)
  const lit = active === null && (tool === `files` || tool === `source-control`)
  const pick = (next: Tool) => () => {
    setOpen(false)
    setTool(next)
  }
  return (
    <div className="ide-rail-computer">
      <button
        className={`ide-rail-sq${lit ? ` is-active` : ``}${interactive ? ` is-click` : ``}`}
        type="button"
        title="Computer"
        aria-label="Computer"
        aria-expanded={open}
        onClick={interactive ? () => setOpen((v) => !v) : undefined}
      >
        <IcComputer size={12.25} />
      </button>
      {open && (
        <div className="ide-menu is-up" role="menu">
          <span className="ide-menu-item" role="menuitem">
            <IcTerminal size={14} />
            Terminal
          </span>
          <button className="ide-menu-item is-click" type="button" role="menuitem" onClick={pick(`files`)}>
            <IcFiles size={14} />
            Files
          </button>
          <button
            className="ide-menu-item is-click"
            type="button"
            role="menuitem"
            onClick={pick(`source-control`)}
          >
            <IcSourceControl size={14} />
            Source Control
          </button>
        </div>
      )}
    </div>
  )
}

/* The fixed header (`render_left_column_header`). */
function LeftHeader() {
  return (
    <div className="ide-left-head">
      <div className="ide-left-headrow">
        <span className="ide-rail-team">
          <span className="ide-rail-teamavatar">{TEAM_NAME[0]}</span>
          <span className="ide-rail-teamname">{TEAM_NAME}</span>
          <IcChevsUpDown size={10.5} className="ide-c-muted" />
        </span>
        <span className="ide-circle-btn" title="Search">
          <IcSearch size={12.25} />
        </span>
        <span className="ide-circle-btn is-primary" title="New issue">
          <IcCreateIssue size={12.25} />
        </span>
      </div>
      <div className="ide-left-rule" />
    </div>
  )
}

export function Rail() {
  const { tool, setTool, active, interactive, inboxRead, thread, openThread, threadKey } = useIde()
  const go = (next: Tool) => (interactive ? () => setTool(next) : undefined)
  /* An entry reads selected while ITS screen is up (EXP-851). */
  const on = (t: Tool) => tool === t && active === null
  const unreadInbox = INBOX_ITEMS.some((n) => n.unread && !inboxRead.has(n.id))
  const live = thread && thread.state !== `ended` ? thread : null
  /* `threads_badge`: amber while a thread needs you, green while one runs. */
  const threadsDot =
    live?.state === `needs_you` ? `is-amber` : live?.state === `working` ? `is-green` : null

  return (
    <div className="ide-left">
      <div className="ide-left-strip">
        <span className="ide-lights" aria-hidden>
          <i style={{ background: `#ff5f57` }} />
          <i style={{ background: `#febc2e` }} />
          <i style={{ background: `#28c840` }} />
        </span>
      </div>
      <LeftHeader />
      <div className="ide-rail">
        <RailRow Icon={IcAgent} label="Agent" active={on(`agent`)} onClick={go(`agent`)} />
        <RailRow
          Icon={IcInbox}
          label="Inbox"
          active={on(`inbox`)}
          /* `inbox_badge`: the PRIMARY dot while anything is unread. */
          badge={unreadInbox ? <span className="ide-rail-dot is-primary" /> : undefined}
          onClick={go(`inbox`)}
        />
        <RailRow
          Icon={IcThreads}
          label="Threads"
          active={on(`threads`)}
          badge={threadsDot ? <span className={`ide-rail-dot ${threadsDot}`} /> : undefined}
          onClick={go(`threads`)}
        />
        <RailRow Icon={IcMonitor} label="Devices" />

        <div className="ide-rail-gap" />
        <SectionLabel label="Agents" />
        {RAIL_AGENTS.map((agent) => (
          <RailRow key={agent.name} Icon={AGENT_GLYPH[agent.icon]} tint={agent.color} label={agent.name} />
        ))}

        <div className="ide-rail-gap" />
        <SectionLabel label="Apps" />
        {/* VAPP-110: the team's apps (no built-in ones). */}
        <RailRow Icon={IcRocket} tint="#f97316" label="Release checklist" />

        <div className="ide-rail-gap" />
        <SectionLabel label="Boards" />
        {BOARDS.map((board, i) => (
          <RailRow
            key={board.name}
            Icon={board.Icon}
            tint={board.color}
            label={board.name}
            active={i === 0 && on(`board`)}
            onClick={i === 0 ? go(`board`) : undefined}
          />
        ))}

        {live && (
          <>
            <div className="ide-rail-gap" />
            <SectionLabel label="Active" plus={false} />
            <button
              className={`ide-runrow${active === threadKey ? ` is-active` : ``}${interactive ? ` is-click` : ``}`}
              type="button"
              onClick={interactive ? openThread : undefined}
            >
              <RunMark needsYou={live.state === `needs_you`} />
              <span className="ide-runrow-title">{`#${live.issueId} ${getIssue(live.issueId).title}`}</span>
              <IcMonitor size={10.5} className="ide-c-muted" />
            </button>
          </>
        )}
      </div>
      <div className="ide-rail-foot">
        <RailRow Icon={IcGettingStarted} label="Getting started" muted />
        <div className="ide-rail-account">
          <span className="ide-rail-me">
            <Avatar person={{ initials: `DS`, name: `Danny Strähhuber` }} size={24} />
            <span className="ide-railrow-label">Danny</span>
            <IcChevsUpDown size={10.5} className="ide-c-muted" />
          </span>
          <ComputerMenu />
          <span className={`ide-rail-sq`} title="Settings">
            <IcSettings size={12.25} />
          </span>
        </div>
      </div>
    </div>
  )
}
