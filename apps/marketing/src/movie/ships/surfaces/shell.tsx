// surfaces/shell.tsx — the desktop shell as it ships today (EXP-1192 one
// labelled rail, EXP-1294 threads, VAPP-110 Apps, NEW-2 agents): the 44px
// work-tabs band (TitleBar), the ONE sidebar (team header · Agent / Inbox /
// Threads / Devices · Agents · Apps · Boards · Active · Getting started ·
// the account footer) and the cutout panel the main view renders into.
//
// Pixel truth: shots/{board,issue-detail,session,thread}/desktop.webp
// (1800×1125 = the 1440×900 window at 1.25) and the desktop crates
// (crates/ui/src/sidebar.rs, app_title_bar.rs, shell.rs; theme FONT_SIZE_PX
// 14, so text_sm = 12.25, text_xs = 10.5, gap_1 = 3.5, px_2 = 7).
//
// Everything here is authored in the app's LOGICAL px (the 1440×900 window)
// and scaled by K onto the film's 1568×980 window box, so every metric is a
// number read straight off the shot (shot px / 1.25) or the Rust source.

import React from "react"
import { interpolate, spring } from "remotion"
import {
  ChartLine,
  ChevronsUpDown,
  Code,
  Computer,
  Inbox,
  ListTree,
  Megaphone,
  MessageCircle,
  Monitor,
  Package,
  Palette,
  Plus,
  Rocket,
  Search,
  Settings,
  Sparkles,
  SquareKanban,
  SquarePen,
  Bug,
  type LucideIcon,
} from "lucide-react"
import { C, MONO_FONT, POP, UI_FONT, WIN } from "../theme"
import { ClaudeMark } from "../../closedloop/surfaces/agentmarks"

const CLAMP = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const

// ── Units ─────────────────────────────────────────────────────────────────────
/** Logical (1440×900 window) → film window px. */
export const K = WIN.w / 1440
export const L = (n: number): number => n * K

/** desktop theme: FONT_SIZE_PX 14 → the rem ladder. */
export const T = {
  xs: 10.5,
  sm: 12.25,
  base: 14,
  lg: 15.75,
  xl: 17.5,
  xl2: 21,
} as const

/** Glass tokens (crates/theme tokens.generated.rs `glass`). */
export const G = {
  fillSection: `rgba(255,255,255,${10 / 255})`,
  fillRow: `rgba(255,255,255,${15 / 255})`,
  fillCard: `rgba(255,255,255,${15 / 255})`,
  fillPanel: `rgba(255,255,255,${10 / 255})`,
  fillActive: `rgba(255,255,255,${28 / 255})`,
  strokeRow: `rgba(255,255,255,${15 / 255})`,
  strokeSection: `rgba(255,255,255,${20 / 255})`,
  strokeCard: `rgba(255,255,255,${26 / 255})`,
  strokeStrong: `rgba(255,255,255,${31 / 255})`,
  strokeActive: `rgba(255,255,255,${41 / 255})`,
  border: `rgba(255,255,255,${26 / 255})`,
  fg: `#fafafa`,
  muted: `#a1a1a1`,
  primary: `#e5e5e5`,
  primaryFg: `#171717`,
  destructive: `#ff6467`,
  amber: `#facc15`,
} as const

/** The cutout panel, logical window px (shell.rs: 272px column + 10px
 * margin, flush under the 44px band, 10px off the other edges). */
export const PANEL = { x: 282, y: 44, w: 1148, h: 846, r: 12 } as const

/** Panel-local logical point → window-local film px (cursor keys). */
export const panelPt = (x: number, y: number): { x: number; y: number } => ({
  x: L(PANEL.x + 1 + x),
  y: L(PANEL.y + 1 + y),
})
/** Window-local logical point → window-local film px. */
export const winPt = (x: number, y: number): { x: number; y: number } => ({
  x: L(x),
  y: L(y),
})

// ── Icons ─────────────────────────────────────────────────────────────────────
export const Ico: React.FC<{
  I: LucideIcon
  size: number
  color?: string
  sw?: number
}> = ({ I, size, color, sw = 2 }) => (
  <I
    size={size}
    strokeWidth={sw}
    color={color ?? `currentColor`}
    style={{ display: `block`, flex: `none` }}
  />
)

// ── Logical layer: an absolutely placed box authored in logical px ───────────
const Logical: React.FC<{
  x: number
  y: number
  w: number
  h: number
  z?: number
  children: React.ReactNode
}> = ({ x, y, w, h, z, children }) => (
  <div
    style={{
      position: `absolute`,
      left: L(x),
      top: L(y),
      width: w,
      height: h,
      transform: `scale(${K})`,
      transformOrigin: `0 0`,
      zIndex: z,
      fontFamily: UI_FONT,
      color: G.fg,
    }}
  >
    {children}
  </div>
)

// ── TitleBar: traffic lights + the work-tab chips + "+" ───────────────────────
export type ShellTabLead =
  | { kind: `status`; status: `backlog` | `in_progress` | `in_review` | `done` }
  | { kind: `running` } // session-card `working` = coding-running (play), fg
  | { kind: `waiting` } // session-card `waiting` = coding-assistant (sparkles), muted
  | { kind: `none` }

export type ShellTab = {
  id: string
  lead: ShellTabLead
  identifier?: string
  title: string
  /** global frame the chip pops in (hidden before) */
  popAt?: number
}

export const StatusGlyph: React.FC<{
  status: `backlog` | `in_progress` | `in_review` | `done`
  size: number
}> = ({ status, size }) => {
  const color =
    status === `backlog`
      ? C.statusBacklog
      : status === `in_progress`
        ? C.statusInProgress
        : status === `in_review`
          ? C.statusInReview
          : C.statusDone
  // icons.json: circle-dashed · progress-2-4 · progress-3-4 · circle-check
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke={color}
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      style={{ display: `block`, flex: `none` }}
    >
      {status === `backlog` ? (
        <>
          <path d="M10.1 2.182a10 10 0 0 1 3.8 0" />
          <path d="M13.9 21.818a10 10 0 0 1-3.8 0" />
          <path d="M17.609 3.721a10 10 0 0 1 2.69 2.7" />
          <path d="M2.182 13.9a10 10 0 0 1 0-3.8" />
          <path d="M20.279 17.609a10 10 0 0 1-2.7 2.69" />
          <path d="M21.818 10.1a10 10 0 0 1 0 3.8" />
          <path d="M3.721 6.391a10 10 0 0 1 2.7-2.69" />
          <path d="M6.391 20.279a10 10 0 0 1-2.69-2.7" />
        </>
      ) : status === `done` ? (
        <>
          <circle cx="12" cy="12" r="10" />
          <path d="m9 12 2 2 4-4" />
        </>
      ) : (
        <>
          <circle cx="12" cy="12" r="10" />
          <path
            d={
              status === `in_progress`
                ? `M12 7 A5 5 0 0 1 12 17 Z`
                : `M12 7 A5 5 0 1 1 7 12 L12 12 Z`
            }
            fill={color}
            stroke="none"
          />
        </>
      )}
    </svg>
  )
}

const PlayGlyph: React.FC<{ size: number }> = ({ size }) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth={2}
    strokeLinecap="round"
    strokeLinejoin="round"
    style={{ display: `block`, flex: `none` }}
  >
    <path d="M5 5a2 2 0 0 1 3.008-1.728l11.997 6.998a2 2 0 0 1 .003 3.458l-12 7A2 2 0 0 1 5 19z" />
  </svg>
)

const TAB = { x: 279, y: 6, h: 32, maxW: 241, gap: 4, padX: 12 } as const

export const TitleBar: React.FC<{
  frame: number
  tabs?: ShellTab[]
  activeId?: string
}> = ({ frame, tabs = [], activeId }) => {
  const shown = tabs.filter((t) => t.popAt === undefined || frame >= t.popAt)
  return (
    <Logical x={0} y={0} w={1440} h={44} z={20}>
      {[19.5, 48.5, 77].map((cx, i) => (
        <span
          key={cx}
          style={{
            position: `absolute`,
            left: cx / 1.25 - 7,
            top: 19.5 / 1.25 - 7,
            width: 14,
            height: 14,
            borderRadius: 999,
            backgroundColor: [`#ff5f57`, `#febc2e`, `#28c840`][i],
          }}
        />
      ))}
      <div
        style={{
          position: `absolute`,
          left: TAB.x,
          top: TAB.y,
          height: TAB.h,
          display: `flex`,
          alignItems: `center`,
          gap: TAB.gap,
        }}
      >
        {shown.map((t) => {
          const pop =
            t.popAt === undefined
              ? 1
              : spring({ frame: frame - t.popAt, fps: 30, config: POP })
          const active = t.id === activeId
          return (
            <div
              key={t.id}
              style={{
                maxWidth: TAB.maxW,
                height: TAB.h,
                boxSizing: `border-box`,
                display: `flex`,
                alignItems: `center`,
                gap: 8,
                padding: `0 ${TAB.padX}px`,
                borderRadius: 10,
                border: `1px solid ${active ? G.strokeActive : `transparent`}`,
                backgroundColor: active ? G.fillActive : `transparent`,
                boxShadow: active ? `0 1px 2px rgba(0,0,0,0.45)` : undefined,
                scale: String(0.8 + 0.2 * Math.min(1, pop)),
                opacity: Math.min(1, pop * 1.4),
              }}
            >
              {t.lead.kind === `status` ? (
                <StatusGlyph status={t.lead.status} size={12} />
              ) : t.lead.kind === `running` ? (
                <span style={{ color: G.fg, display: `flex` }}>
                  <PlayGlyph size={12} />
                </span>
              ) : t.lead.kind === `waiting` ? (
                <Ico I={Sparkles} size={12} color={G.muted} />
              ) : null}
              {t.identifier ? (
                <span
                  style={{
                    fontFamily: MONO_FONT,
                    fontSize: T.xs,
                    color: G.muted,
                    whiteSpace: `nowrap`,
                  }}
                >
                  {t.identifier}
                </span>
              ) : null}
              <span
                style={{
                  minWidth: 0,
                  fontSize: T.sm,
                  color: active ? G.fg : G.muted,
                  whiteSpace: `nowrap`,
                  overflow: `hidden`,
                  textOverflow: `ellipsis`,
                }}
              >
                {t.title}
              </span>
            </div>
          )
        })}
        <span
          style={{
            width: 24,
            height: 24,
            marginLeft: shown.length > 0 ? 3 : 0,
            display: `flex`,
            alignItems: `center`,
            justifyContent: `center`,
            color: G.muted,
          }}
        >
          <Ico I={Plus} size={12} />
        </span>
      </div>
    </Logical>
  )
}

// ── CutoutPanel: children are PANEL-LOCAL LOGICAL px ─────────────────────────
export const CutoutPanel: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => (
  <Logical x={PANEL.x} y={PANEL.y} w={PANEL.w} h={PANEL.h} z={5}>
    <div
      style={{
        position: `absolute`,
        inset: 0,
        boxSizing: `border-box`,
        borderRadius: PANEL.r,
        border: `1px solid ${G.strokeCard}`,
        backgroundColor: G.fillPanel,
        overflow: `hidden`,
      }}
    >
      <div style={{ position: `absolute`, left: 0, top: 0, width: PANEL.w - 2, height: PANEL.h - 2 }}>
        {children}
      </div>
    </div>
  </Logical>
)

// ── Sidebar ───────────────────────────────────────────────────────────────────
export type SidebarItem = {
  id: string
  label: string
  I: LucideIcon
  color?: string
}

export type ActiveThread = {
  id: string
  title: string
  /** global frame the row pops in */
  at?: number
  /** amber unread dot on the agent mark */
  unread?: boolean
}

export type SidebarProps = {
  frame: number
  team: string
  user: string
  initials: string
  /** the selected row: a nav id (`agent`, `inbox`, `threads`, `devices`),
   * `board:<name>` or `thread:<id>` */
  selected?: string
  /** global frame the Inbox unread dot appears */
  inboxDotAt?: number
  boards: { name: string; glyph: `code` | `kanban` | `megaphone`; color: string }[]
  active?: ActiveThread[]
}

const NAV: SidebarItem[] = [
  { id: `agent`, label: `Agent`, I: MessageCircle },
  { id: `inbox`, label: `Inbox`, I: Inbox },
  { id: `threads`, label: `Threads`, I: ListTree },
  { id: `devices`, label: `Devices`, I: Monitor },
]
// The team's agent definitions and apps (the screenshot seed's rows).
const AGENTS: SidebarItem[] = [
  { id: `triage`, label: `Triage`, I: Bug, color: `#a855f7` },
  { id: `designer`, label: `Designer`, I: Palette, color: `#ec4899` },
  { id: `release`, label: `Release`, I: Package, color: `#22c55e` },
]
const APPS: SidebarItem[] = [
  { id: `app-devices`, label: `Devices`, I: Monitor },
  { id: `app-release`, label: `Release checklist`, I: Rocket, color: `#f59e0b` },
  { id: `app-dashboard`, label: `Team dashboard`, I: ChartLine, color: `#0ea5e9` },
]
const BOARD_ICON = { code: Code, kanban: SquareKanban, megaphone: Megaphone } as const

const ROW_H = 28
const ROW_GAP = 3.5
const SECTION_RULE = 8
const HEADER_H = 24
const COL_X = 7 // px_2
const COL_W = 282 - 10 - 2 * COL_X // the column inside px_2 (rail 272)

/** Logical y of each sidebar row's top (for cursor aim). */
export const sidebarLayout = (activeCount: number) => {
  const tops: Record<string, number> = {}
  let y = 85.2
  const row = (id: string) => {
    tops[id] = y
    y += ROW_H + ROW_GAP
  }
  const section = (id: string) => {
    y += SECTION_RULE + ROW_GAP
    tops[id] = y
    y += HEADER_H + ROW_GAP
  }
  NAV.forEach((n) => row(n.id))
  section(`h:agents`)
  AGENTS.forEach((n) => row(n.id))
  section(`h:apps`)
  APPS.forEach((n) => row(n.id))
  section(`h:boards`)
  for (let i = 0; i < 3; i++) row(`board:${i}`)
  if (activeCount > 0) {
    section(`h:active`)
    for (let i = 0; i < activeCount; i++) row(`active:${i}`)
  }
  return tops
}

/** Window-local film px of a sidebar row's center. */
export const sidebarRowCenter = (
  key: string,
  activeCount: number
): { x: number; y: number } => {
  const top = sidebarLayout(activeCount)[key] ?? 0
  return winPt(COL_X + 70, top + ROW_H / 2)
}

const Row: React.FC<{
  selected?: boolean
  lead: React.ReactNode
  label: string
  trailing?: React.ReactNode
  labelColor?: string
  indent?: number
}> = ({ selected, lead, label, trailing, labelColor = G.fg }) => (
  <div
    style={{
      height: ROW_H,
      boxSizing: `border-box`,
      display: `flex`,
      alignItems: `center`,
      gap: 8,
      padding: `0 7px`,
      borderRadius: 10,
      backgroundColor: selected ? G.fillActive : undefined,
      flex: `none`,
    }}
  >
    {lead}
    <span
      style={{
        flex: 1,
        minWidth: 0,
        fontSize: T.sm,
        fontWeight: 500,
        color: labelColor,
        whiteSpace: `nowrap`,
        overflow: `hidden`,
        textOverflow: `ellipsis`,
      }}
    >
      {label}
    </span>
    {trailing}
  </div>
)

const SectionHeader: React.FC<{ label: string; plus?: boolean }> = ({
  label,
  plus = true,
}) => (
  <div
    style={{
      height: HEADER_H,
      display: `flex`,
      alignItems: `center`,
      padding: `0 7px 0 5.8px`,
      fontSize: T.xs,
      fontWeight: 600,
      color: G.muted,
      flex: `none`,
    }}
  >
    <span style={{ flex: 1 }}>{label}</span>
    {plus ? <Ico I={Plus} size={12} color={G.muted} /> : null}
  </div>
)

const Gap: React.FC<{ h: number }> = ({ h }) => (
  <div style={{ height: h, flex: `none` }} />
)

export const Sidebar: React.FC<SidebarProps> = ({
  frame,
  team,
  user,
  initials,
  selected,
  inboxDotAt,
  boards,
  active = [],
}) => {
  const liveRows = active.filter((a) => a.at === undefined || frame >= a.at)
  const rows: React.ReactNode[] = []
  const push = (n: React.ReactNode) => rows.push(n)
  const gap = () => push(<Gap key={`g${rows.length}`} h={ROW_GAP} />)
  const rule = () => {
    push(<Gap key={`r${rows.length}`} h={SECTION_RULE} />)
    gap()
  }

  NAV.forEach((n, i) => {
    if (i > 0) gap()
    const dot =
      n.id === `inbox` && inboxDotAt !== undefined && frame >= inboxDotAt ? (
        <span
          style={{
            width: 6,
            height: 6,
            marginRight: 2,
            borderRadius: 999,
            backgroundColor: `#e5e5e5`,
            scale: String(
              Math.min(1, spring({ frame: frame - inboxDotAt, fps: 30, config: POP }))
            ),
          }}
        />
      ) : null
    push(
      <Row
        key={n.id}
        selected={selected === n.id}
        lead={<Ico I={n.I} size={14} />}
        label={n.label}
        trailing={dot}
      />
    )
  })
  gap()
  rule()
  push(<SectionHeader key="h-agents" label="Agents" />)
  for (const a of AGENTS) {
    gap()
    push(<Row key={a.id} lead={<Ico I={a.I} size={14} color={a.color} />} label={a.label} />)
  }
  gap()
  rule()
  push(<SectionHeader key="h-apps" label="Apps" />)
  for (const a of APPS) {
    gap()
    push(<Row key={a.id} lead={<Ico I={a.I} size={14} color={a.color} />} label={a.label} />)
  }
  gap()
  rule()
  push(<SectionHeader key="h-boards" label="Boards" />)
  boards.forEach((b) => {
    gap()
    push(
      <Row
        key={b.name}
        selected={selected === `board:${b.name}`}
        lead={<Ico I={BOARD_ICON[b.glyph]} size={14} color={b.color} />}
        label={b.name}
      />
    )
  })
  if (liveRows.length > 0) {
    gap()
    rule()
    push(<SectionHeader key="h-active" label="Active" plus={false} />)
    liveRows.forEach((a) => {
      gap()
      const pop =
        a.at === undefined
          ? 1
          : interpolate(frame, [a.at, a.at + 10], [0, 1], CLAMP)
      push(
        <div key={a.id} style={{ opacity: pop, flex: `none` }}>
          <Row
            selected={selected === `thread:${a.id}`}
            lead={
              <span style={{ position: `relative`, display: `flex`, marginLeft: 1 }}>
                <ClaudeMark size={13} />
                {a.unread ? (
                  <span
                    style={{
                      position: `absolute`,
                      top: -2,
                      right: -2,
                      width: 5,
                      height: 5,
                      borderRadius: 999,
                      backgroundColor: G.amber,
                    }}
                  />
                ) : null}
              </span>
            }
            label={a.title}
            trailing={<Ico I={Monitor} size={10.5} color={G.muted} />}
          />
        </div>
      )
    })
  }

  return (
    <Logical x={0} y={0} w={282} h={900} z={10}>
      {/* the team header: avatar square · name · switcher · search · compose */}
      <div
        style={{
          position: `absolute`,
          left: 12,
          top: 40,
          width: 28,
          height: 28,
          borderRadius: 6.5,
          backgroundColor: `#d4d4d4`,
          color: `#171717`,
          display: `flex`,
          alignItems: `center`,
          justifyContent: `center`,
          fontSize: T.sm,
          fontWeight: 600,
        }}
      >
        {team.charAt(0)}
      </div>
      <div
        style={{
          position: `absolute`,
          left: 48,
          top: 44,
          height: 20,
          display: `flex`,
          alignItems: `center`,
          fontSize: T.sm,
          fontWeight: 600,
        }}
      >
        {team}
      </div>
      <span style={{ position: `absolute`, left: 183 - 6, top: 53.6 - 6, color: G.muted }}>
        <Ico I={ChevronsUpDown} size={12} />
      </span>
      <span style={{ position: `absolute`, left: 213 - 7, top: 53.6 - 7, color: G.fg }}>
        <Ico I={Search} size={14} />
      </span>
      <div
        style={{
          position: `absolute`,
          left: 249 - 16,
          top: 53.6 - 16,
          width: 32,
          height: 32,
          borderRadius: 999,
          backgroundColor: G.primary,
          color: G.primaryFg,
          display: `flex`,
          alignItems: `center`,
          justifyContent: `center`,
        }}
      >
        <Ico I={SquarePen} size={14} />
      </div>
      <div
        style={{
          position: `absolute`,
          left: 6.4,
          width: 259.2,
          top: 80.6,
          height: 1,
          backgroundColor: G.border,
        }}
      />

      {/* the scrolling middle */}
      <div
        style={{
          position: `absolute`,
          left: COL_X,
          top: 85.2,
          width: COL_W,
          height: 820 - 85.2,
          display: `flex`,
          flexDirection: `column`,
          overflow: `hidden`,
        }}
      >
        {rows}
      </div>

      {/* footer: Getting started · the account row */}
      <div style={{ position: `absolute`, left: COL_X, top: 826, width: COL_W }}>
        <Row
          lead={<Ico I={Sparkles} size={14} color={G.muted} />}
          label="Getting started"
          labelColor={G.muted}
        />
      </div>
      <div
        style={{
          position: `absolute`,
          left: COL_X,
          top: 858,
          width: COL_W,
          height: 36,
          boxSizing: `border-box`,
          display: `flex`,
          alignItems: `center`,
          gap: 8,
          padding: `0 5.25px`,
        }}
      >
        <span
          style={{
            width: 24,
            height: 24,
            borderRadius: 999,
            backgroundColor: `rgba(234,179,8,0.22)`,
            color: `#facc15`,
            fontSize: 9,
            fontWeight: 600,
            display: `flex`,
            alignItems: `center`,
            justifyContent: `center`,
            flex: `none`,
          }}
        >
          {initials}
        </span>
        <span style={{ flex: 1, fontSize: T.sm, fontWeight: 500 }}>{user}</span>
        <span style={{ color: G.muted, display: `flex`, marginRight: 9 }}>
          <Ico I={ChevronsUpDown} size={12} />
        </span>
        <span style={{ display: `flex`, marginRight: 9 }}>
          <Ico I={Computer} size={13} />
        </span>
        <span style={{ display: `flex`, marginRight: 4 }}>
          <Ico I={Settings} size={13} />
        </span>
      </div>
    </Logical>
  )
}
