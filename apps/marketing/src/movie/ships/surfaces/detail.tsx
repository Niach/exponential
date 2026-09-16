// surfaces/detail.tsx — IssueDetailPane: the ISSUE face of a top tab, and the
// ONE WorkHeader both faces share (EXP-877; pixel truth
// shots/issue-detail/desktop.webp + crates/ui/src/work_header.rs):
//   · the fixed header, capped to the work column: the 24/32 semibold title
//     with the right cluster top-aligned beside it — the `Issue | Run | +N -M`
//     face toggle (hidden until a run exists), the pin, the `…` menu
//   · the properties TRAY under it (status · priority · assignee · label ·
//     due · board · origin — glass pills in a section-filled card) with the
//     ONE coding action at its right edge: "▷ Start coding", or "⊗ Stop"
//     while my run is live
//   · a hairline, then the body in the same column: the markdown
//     description, the emoji / image / attach affordance row, a full-bleed
//     hairline, "Activity (n)", the timeline (muted event lines ending in
//     their time, comment CARDS on the 28px gutter rail that each close with
//     a "Leave a reply…" row, EXP-723/741) and the composer.
// EXP-723 raised the app rem 13 → 14, so the transcribed text sizes below
// carry the 14/13 step (12 → 13, 13 → 14, 23 → 25).
// All frames are composition-global; the assembler passes `frame` down (no
// useCurrentFrame here).
//
// Coordinates: the pane lays out in PANE-LOCAL px and fills the cutout
// panel's main view (default WIN.panel size). `bodyOnly` drops the header so
// a caller flipping faces can keep ONE header over a sliding body.

import React from "react"
import { interpolate, spring } from "remotion"
import { C, EASE, MONO_FONT, POP, UI_FONT, WIN } from "../theme"
import { BOARD, HERO, IDENTITY, LABELS } from "../fixtures"
import type { IssueStatus, Priority } from "../fixtures"

const CLAMP = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const
const CLAMP_EASE = { ...CLAMP, easing: EASE } as const

// Contract-sanctioned literals for this surface (matched on the ref shot).
const DESC_FG = "#d4d4d4" // body paragraph color per contract
const PRIMARY_BG = "#ededed" // the light Start coding / New Issue pill
const PRIMARY_FG = "#18181b"

// ── Layout constants (pane-local) ────────────────────────────────────────────
const DEFAULT_W = WIN.panel.w
const DEFAULT_H = WIN.panel.h
const PROSE_W = 600

// ── Tiny inline icons (lucide-like, stroke currentColor) ─────────────────────
type IconProps = { size?: number; sw?: number; style?: React.CSSProperties }
const Svg: React.FC<IconProps & { children: React.ReactNode }> = ({
  size = 14,
  sw = 2,
  style,
  children,
}) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth={sw}
    strokeLinecap="round"
    strokeLinejoin="round"
    style={{ display: "block", flexShrink: 0, ...style }}
  >
    {children}
  </svg>
)

const IcPlay: React.FC<IconProps> = (p) => (
  <Svg {...p}>
    <path d="M7 5.5 18.5 12 7 18.5Z" />
  </Svg>
)
const IcCircleX: React.FC<IconProps> = (p) => (
  <Svg {...p}>
    <circle cx="12" cy="12" r="9" />
    <path d="m15 9-6 6" />
    <path d="m9 9 6 6" />
  </Svg>
)
// relation-section = link-2, ui-add = plus (icons.json)
// issue_header::render_actions_menu — the round "…" the top row now is.
const IcEllipsis: React.FC<IconProps> = (p) => (
  <Svg {...p} sw={2}>
    <circle cx="12" cy="12" r="1" />
    <circle cx="19" cy="12" r="1" />
    <circle cx="5" cy="12" r="1" />
  </Svg>
)
const IcTag: React.FC<IconProps> = (p) => (
  <Svg {...p} sw={1.8}>
    <path d="M12.586 2.586A2 2 0 0 0 11.172 2H4a2 2 0 0 0-2 2v7.172a2 2 0 0 0 .586 1.414l8.704 8.704a2.426 2.426 0 0 0 3.42 0l6.58-6.58a2.426 2.426 0 0 0 0-3.42z" />
    <circle cx="7.5" cy="7.5" r="0.8" />
  </Svg>
)
const IcCalendarDays: React.FC<IconProps> = (p) => (
  <Svg {...p} sw={1.7}>
    <path d="M8 2v4" />
    <path d="M16 2v4" />
    <rect x="3" y="4" width="18" height="18" rx="2" />
    <path d="M3 10h18" />
    <path d="M8 14h.01" />
    <path d="M12 14h.01" />
    <path d="M16 14h.01" />
    <path d="M8 18h.01" />
    <path d="M12 18h.01" />
  </Svg>
)
const IcCircleUser: React.FC<IconProps> = (p) => (
  <Svg {...p} sw={1.8}>
    <circle cx="12" cy="12" r="10" />
    <circle cx="12" cy="10" r="3" />
    <path d="M6.2 19.4a6.5 6.5 0 0 1 11.6 0" />
  </Svg>
)
const IcCode: React.FC<IconProps> = (p) => (
  <Svg {...p}>
    <path d="m16 18 6-6-6-6" />
    <path d="m8 6-6 6 6 6" />
  </Svg>
)
// EXP-496 origin chip: `MessageSquare` for a widget-filed issue.
const IcMessageSquare: React.FC<IconProps> = (p) => (
  <Svg {...p}>
    <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
  </Svg>
)

const IcSmile: React.FC<IconProps> = (p) => (
  <Svg {...p} sw={1.7}>
    <circle cx="12" cy="12" r="10" />
    <path d="M8 14s1.5 2 4 2 4-2 4-2" />
    <path d="M9 9h.01" />
    <path d="M15 9h.01" />
  </Svg>
)
const IcImage: React.FC<IconProps> = (p) => (
  <Svg {...p} sw={1.7}>
    <rect x="3" y="3" width="18" height="18" rx="2" />
    <circle cx="9" cy="9" r="2" />
    <path d="m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21" />
  </Svg>
)
const IcPaperclip: React.FC<IconProps> = (p) => (
  <Svg {...p} sw={1.7}>
    <path d="M13.234 20.252 21 12.3a3.8 3.8 0 0 0-5.373-5.374l-9.02 9.148a5.7 5.7 0 0 0 8.06 8.06l8.535-8.535" />
  </Svg>
)
// Priority: signal-low / -medium / -high (baseline dot + ascending bars) and
// triangle-alert for urgent (icons.json priority-*).
const IcSignal: React.FC<IconProps & { bars: 1 | 2 | 3 }> = ({ bars, ...p }) => (
  <Svg {...p}>
    <path d="M2 20h.01" />
    <path d="M7 20v-4" />
    {bars >= 2 ? <path d="M12 20v-8" /> : null}
    {bars >= 3 ? <path d="M17 20V8" /> : null}
  </Svg>
)
const IcSignalLow: React.FC<IconProps> = (p) => <IcSignal {...p} bars={1} />
const IcSignalMedium: React.FC<IconProps> = (p) => <IcSignal {...p} bars={2} />
const IcSignalHigh: React.FC<IconProps> = (p) => <IcSignal {...p} bars={3} />
const IcTriangleAlert: React.FC<IconProps> = (p) => (
  <Svg {...p}>
    <path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 20h16a2 2 0 0 0 1.73-2Z" />
    <path d="M12 9v4" />
    <path d="M12 17h.01" />
  </Svg>
)
const IcMinus: React.FC<IconProps> = (p) => (
  <Svg {...p}>
    <path d="M5 12h14" />
  </Svg>
)
// Status glyphs
const IcCircleDashed: React.FC<IconProps> = (p) => (
  <Svg {...p}>
    <circle cx="12" cy="12" r="9" strokeDasharray="3.6 3.4" />
  </Svg>
)
// Pie-clock started glyph (icons.json progress-2-4 — builtin In Progress).
const IcPieClock: React.FC<IconProps> = (p) => (
  <Svg {...p}>
    <circle cx="12" cy="12" r="10" />
    <path
      d="M12 12 L12 6 A6 6 0 0 1 12 18 Z"
      fill="currentColor"
      stroke="none"
    />
  </Svg>
)
const IcCircleCheck: React.FC<IconProps> = (p) => (
  <Svg {...p}>
    <circle cx="12" cy="12" r="9" />
    <path d="m8.5 12 2.5 2.5 5-5" />
  </Svg>
)

const STATUS_META: Record<
  IssueStatus,
  { label: string; color: string; Icon: React.FC<IconProps> }
> = {
  backlog: { label: "Backlog", color: C.statusBacklog, Icon: IcCircleDashed },
  in_progress: {
    label: "In Progress",
    color: C.statusInProgress,
    Icon: IcPieClock,
  },
  done: { label: "Done", color: C.statusDone, Icon: IcCircleCheck },
}
const PRIO_META: Record<
  Priority,
  { label: string; color: string; Icon: React.FC<IconProps> }
> = {
  none: { label: "No priority", color: C.muted, Icon: IcMinus },
  urgent: { label: "Urgent", color: C.prioUrgent, Icon: IcTriangleAlert },
  high: { label: "High", color: C.prioHigh, Icon: IcSignalHigh },
  medium: { label: "Medium", color: C.prioMedium, Icon: IcSignalMedium },
  low: { label: "Low", color: C.prioLow, Icon: IcSignalLow },
}

// ── Small shared bits ─────────────────────────────────────────────────────────
const popIn = (frame: number, at: number | undefined) =>
  at === undefined || frame < at
    ? 0
    : spring({ frame: frame - at, fps: 30, config: POP })

// One property chip in the tray: `glass_pill` at the Sm rung (24px capsule,
// hairline stroke) carrying a colored glyph + its value (pickers.rs
// `chip_button` / surface.rs `glass_pill`).
const Prop: React.FC<{
  Icon: React.FC<IconProps>
  color?: string
  children: React.ReactNode
}> = ({ Icon, color = C.muted, children }) => (
  <div
    style={{
      height: 24,
      boxSizing: "border-box",
      display: "flex",
      alignItems: "center",
      gap: 5,
      padding: "0 9px",
      borderRadius: 999,
      border: `1px solid ${C.strokeCard}`,
      backgroundColor: C.fillCard,
      flex: "none",
    }}
  >
    <Icon size={13} style={{ color }} />
    <span
      style={{
        fontSize: 13,
        color: C.text,
        whiteSpace: "nowrap",
      }}
    >
      {children}
    </span>
  </div>
)

// timeline::timeline_row — the 28px gutter with the row's marker centred and
// a 1px card-stroke rail above and below it, broken 6px around the marker.
const GUTTER = 28
const RAIL_BREAK = 6
const TimelineRow: React.FC<{
  marker: React.ReactNode
  markerTop: number
  markerSize: number
  padY?: number
  first?: boolean
  last?: boolean
  children: React.ReactNode
}> = ({ marker, markerTop, markerSize, padY = 3.5, first, last, children }) => {
  const aboveH = markerTop + padY - RAIL_BREAK
  const belowTop = markerTop + markerSize + RAIL_BREAK
  const rail: React.CSSProperties = {
    position: "absolute",
    left: GUTTER / 2 - 0.5,
    width: 1,
    backgroundColor: C.strokeCard,
  }
  return (
    <div style={{ display: "flex", gap: 7, paddingTop: padY, paddingBottom: padY }}>
      <div style={{ position: "relative", width: GUTTER, flex: "none" }}>
        {!first && aboveH > 0 ? (
          <span style={{ ...rail, top: -padY, height: aboveH }} />
        ) : null}
        {!last ? <span style={{ ...rail, top: belowTop, bottom: -padY }} /> : null}
        <div
          style={{
            position: "relative",
            marginTop: markerTop,
            height: markerSize,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
          }}
        >
          {marker}
        </div>
      </div>
      <div style={{ flex: 1, minWidth: 0 }}>{children}</div>
    </div>
  )
}

// ── Issue content shown by the pane ──────────────────────────────────────────
export type DetailComment = {
  actor: string
  initials: string
  time: string
  body: string
}

export type DetailIssueContent = {
  id: string
  title: string
  descriptionParas: readonly string[]
  switcher: string
  activity: readonly { actor: string; text: string; time?: string }[]
  comments?: readonly DetailComment[]
  imagesMeta?: string // legacy field — the current detail has no images meta row
  pr?: number
  label?: { name: string; dot: string }
  assigneeName?: string
  due?: string
  board?: string
  boardColor?: string
  /** EXP-496 origin chip label — `Feedback widget` / `Agent`, else none. */
  origin?: string
}

const HERO_ISSUE: DetailIssueContent = {
  id: HERO.id,
  title: HERO.title,
  descriptionParas: HERO.descriptionParas,
  switcher: HERO.switcher,
  activity: HERO.activity,
  pr: HERO.pr,
  label: LABELS.bug,
  assigneeName: IDENTITY.user,
  due: BOARD.find((r) => r.id === HERO.id)?.due ?? "Jul 15",
  board: IDENTITY.board,
  boardColor: IDENTITY.boardColor,
}

// ── The ONE work header (EXP-877 work_header.rs, web `WorkHeader`) ───────────
// Shared by the issue face and the run face of a top tab, fixed above the
// body and capped to the 896px work column. Row 1: the 24/32 semibold title
// with the right cluster top-aligned on the same line — the face toggle
// (`Issue | Run | +N -M`, HIDDEN below two items), the pin, the `…` menu.
// Row 2: the properties TRAY (glass_tray: 12/8 insets, 6 gap, section fill
// + card hairline) with the ONE coding action at its right edge — Start
// coding, or Stop while my run is live. A hairline closes the header.
// The app's 896px `WORK_COLUMN_W` at the film's 1.09 window step.
export const WORK_COLUMN_W = 976
export const DETAIL_GUTTER = 16
const TITLE_PT = 16
const TITLE_LINE = 32
const TITLE_PB = 4
const TRAY_H = 42
const HEADER_PB = 12
export const WORK_HEADER_H =
  TITLE_PT + TITLE_LINE + TITLE_PB + TRAY_H + HEADER_PB + 1

const IcPin: React.FC<IconProps> = (p) => (
  <Svg {...p} sw={1.8}>
    <path d="M12 17v5" />
    <path d="M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H8a2 2 0 0 0 0 4 1 1 0 0 1 1 1z" />
  </Svg>
)

export type WorkFace = `issue` | `run`

export type WorkHeaderProps = {
  frame: number
  issue: DetailIssueContent
  status: IssueStatus
  priority: Priority
  width: number
  /** Global frame a run of mine exists from — the toggle pops in. */
  runAt?: number
  /** Global frame the active face flips Issue → Run (the pill slides). */
  runFaceAt?: number
  /** The run's diff item (`+N -M`), from its global frame. */
  diff?: { add: number; del: number; at: number }
  /** My run is live: Start coding → Stop. */
  codingNow?: { at: number; out?: number }
}

const FACE_ISSUE_W = 58
const FACE_RUN_W = 50
const faceDiffW = (add: number, del: number) =>
  24 + Math.round((`+${add}`.length + `-${del}`.length) * 7.8) + 4

export const WorkHeader: React.FC<WorkHeaderProps> = ({
  frame,
  issue,
  status,
  priority,
  width,
  runAt,
  runFaceAt,
  diff,
  codingNow,
}) => {
  const st = STATUS_META[status]
  const pr = PRIO_META[priority]
  const colW = Math.min(WORK_COLUMN_W, width)
  const codingActive =
    codingNow !== undefined &&
    frame >= codingNow.at &&
    (codingNow.out === undefined || frame < codingNow.out + 4)
  const togglePop = popIn(frame, runAt)
  const diffPop = popIn(frame, diff?.at)
  const faceT =
    runFaceAt === undefined
      ? 0
      : interpolate(frame, [runFaceAt, runFaceAt + 8], [0, 1], CLAMP_EASE)
  const showToggle = runAt !== undefined && frame >= runAt
  const showDiff = showToggle && diff !== undefined && frame >= diff.at

  const faceItem = (
    label: React.ReactNode,
    w: number,
    active: boolean
  ): React.ReactElement => (
    <span
      style={{
        position: `relative`,
        width: w,
        height: `100%`,
        flex: `none`,
        display: `flex`,
        alignItems: `center`,
        justifyContent: `center`,
        fontSize: 14,
        color: active ? C.text : C.muted,
      }}
    >
      {label}
    </span>
  )

  return (
    <div
      style={{
        position: `relative`,
        width,
        height: WORK_HEADER_H,
        boxSizing: `border-box`,
        borderBottom: `1px solid ${C.strokeRow}`,
        fontFamily: UI_FONT,
        color: C.text,
      }}
    >
      <div style={{ width: colW, margin: `0 auto`, position: `relative` }}>
        {/* row 1: the title · the right cluster, top-aligned */}
        <div style={{ display: `flex`, alignItems: `flex-start` }}>
          <div
            style={{
              flex: 1,
              minWidth: 0,
              padding: `${TITLE_PT}px ${DETAIL_GUTTER}px ${TITLE_PB}px`,
              fontSize: 25,
              fontWeight: 600,
              lineHeight: `${TITLE_LINE}px`,
              letterSpacing: -0.3,
              whiteSpace: `nowrap`,
              overflow: `hidden`,
              textOverflow: `ellipsis`,
            }}
          >
            {issue.title}
          </div>
          <div
            style={{
              flex: `none`,
              display: `flex`,
              alignItems: `center`,
              gap: 4,
              paddingTop: TITLE_PT - 2,
              paddingRight: DETAIL_GUTTER,
              color: C.muted,
            }}
          >
            {showToggle ? (
              <div
                style={{
                  position: `relative`,
                  height: 36,
                  boxSizing: `border-box`,
                  display: `flex`,
                  alignItems: `center`,
                  padding: 3,
                  borderRadius: 999,
                  border: `1px solid ${C.strokeSection}`,
                  backgroundColor: C.fillSection,
                  scale: String(0.9 + 0.1 * Math.min(1, togglePop)),
                  opacity: Math.min(1, togglePop * 1.5),
                }}
              >
                {/* the active capsule slides Issue → Run */}
                <span
                  style={{
                    position: `absolute`,
                    top: 3,
                    bottom: 3,
                    left: 3 + faceT * FACE_ISSUE_W,
                    width: FACE_ISSUE_W + (FACE_RUN_W - FACE_ISSUE_W) * faceT,
                    boxSizing: `border-box`,
                    borderRadius: 999,
                    border: `1px solid ${C.strokeActive}`,
                    backgroundColor: C.fillActive,
                  }}
                />
                {faceItem(`Issue`, FACE_ISSUE_W, faceT < 0.5)}
                {faceItem(`Run`, FACE_RUN_W, faceT >= 0.5)}
                {showDiff && diff ? (
                  <span
                    style={{
                      display: `flex`,
                      overflow: `hidden`,
                      width: faceDiffW(diff.add, diff.del) * Math.min(1, diffPop),
                    }}
                  >
                    {faceItem(
                      <span style={{ display: `flex`, gap: 4, fontFamily: MONO_FONT, fontSize: 13 }}>
                        <span style={{ color: C.diffAdd }}>{`+${diff.add}`}</span>
                        <span style={{ color: C.destructive }}>{`-${diff.del}`}</span>
                      </span>,
                      faceDiffW(diff.add, diff.del),
                      false
                    )}
                  </span>
                ) : null}
              </div>
            ) : null}
            <span style={{ width: 30, height: 30, display: `flex`, alignItems: `center`, justifyContent: `center` }}>
              <IcPin size={15} />
            </span>
            <span style={{ width: 30, height: 30, display: `flex`, alignItems: `center`, justifyContent: `center` }}>
              <IcEllipsis size={16} />
            </span>
          </div>
        </div>

        {/* row 2: the properties tray + the ONE coding action */}
        <div style={{ padding: `0 ${DETAIL_GUTTER}px` }}>
          <div
            style={{
              height: TRAY_H,
              boxSizing: `border-box`,
              display: `flex`,
              alignItems: `center`,
              flexWrap: `nowrap`,
              overflow: `hidden`,
              gap: 6,
              padding: `8px 12px`,
              borderRadius: 12,
              border: `1px solid ${C.strokeCard}`,
              backgroundColor: C.fillSection,
            }}
          >
            <Prop Icon={st.Icon} color={st.color}>
              {st.label}
            </Prop>
            <Prop Icon={pr.Icon} color={pr.color}>
              {pr.label}
            </Prop>
            <Prop Icon={IcCircleUser}>{issue.assigneeName ?? `Unassigned`}</Prop>
            {issue.label ? (
              <Prop Icon={IcTag}>
                {issue.label.name.charAt(0).toUpperCase() +
                  issue.label.name.slice(1)}
              </Prop>
            ) : null}
            {issue.due ? <Prop Icon={IcCalendarDays}>{issue.due}</Prop> : null}
            <Prop Icon={IcCode} color={issue.boardColor ?? `#818cf8`}>
              {issue.board ?? IDENTITY.board}
            </Prop>
            {issue.origin ? (
              <Prop Icon={IcMessageSquare}>{issue.origin}</Prop>
            ) : null}
            <span style={{ flex: 1, minWidth: 4 }} />
            {codingActive ? (
              <div
                style={{
                  height: 26,
                  flex: `none`,
                  boxSizing: `border-box`,
                  display: `flex`,
                  alignItems: `center`,
                  gap: 5,
                  padding: `0 10px`,
                  borderRadius: 999,
                  border: `1px solid ${C.strokeCard}`,
                  backgroundColor: C.fillCard,
                  color: C.destructive,
                  fontSize: 13,
                  whiteSpace: `nowrap`,
                }}
              >
                <IcCircleX size={13} />
                Stop
              </div>
            ) : (
              <div
                style={{
                  height: 26,
                  flex: `none`,
                  boxSizing: `border-box`,
                  display: `flex`,
                  alignItems: `center`,
                  gap: 6,
                  padding: `0 12px`,
                  borderRadius: 999,
                  backgroundColor: PRIMARY_BG,
                  color: PRIMARY_FG,
                  fontSize: 13,
                  fontWeight: 500,
                  whiteSpace: `nowrap`,
                }}
              >
                <IcPlay size={12} sw={1.8} />
                Start coding
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  )
}

// ── The pane ──────────────────────────────────────────────────────────────────
export type IssueDetailPaneProps = {
  frame: number
  /** My run is live: the header's Start coding → Stop. */
  codingNow?: { at: number; out?: number }
  /** Render the BODY only — the caller draws the fixed WorkHeader itself
   * (a tab flipping faces keeps ONE header while the body swaps). */
  bodyOnly?: boolean
  /** Whole pane slides in from the right 46px + fades over 20f. */
  slideInAt?: number
  /** Properties STATUS value (board truth changes over the film). */
  status?: IssueStatus
  priority?: Priority
  /** Issue content (title/description/activity/properties). Default: the ships HERO. */
  issue?: DetailIssueContent
  width?: number
  height?: number
}

export const IssueDetailPane: React.FC<IssueDetailPaneProps> = ({
  frame,
  codingNow,
  slideInAt,
  status = "backlog",
  priority = "high",
  issue = HERO_ISSUE,
  width = DEFAULT_W,
  height = DEFAULT_H,
  bodyOnly = false,
}) => {
  const slide =
    slideInAt === undefined
      ? { opacity: 1, translate: "0px 0px" }
      : {
          opacity: interpolate(
            frame,
            [slideInAt, slideInAt + 20],
            [0, 1],
            CLAMP_EASE
          ),
          translate: `${interpolate(
            frame,
            [slideInAt, slideInAt + 20],
            [46, 0],
            CLAMP_EASE
          )}px 0px`,
        }

  // The body shares the header's centred work column (EXP-877); PROSE_W
  // caps the running text so it never runs under the phone the clips float
  // over the window's right edge.
  const colLeft = (width - Math.min(WORK_COLUMN_W, width)) / 2 + DETAIL_GUTTER
  const prose: React.CSSProperties = {
    marginLeft: colLeft,
    width: Math.min(PROSE_W, width - 2 * colLeft),
  }

  return (
    <div
      style={{
        position: "absolute",
        left: 0,
        top: 0,
        width,
        height,
        overflow: "hidden",
        fontFamily: UI_FONT,
        color: C.text,
        opacity: slide.opacity,
        translate: slide.translate,
      }}
    >
      {bodyOnly ? null : (
        <WorkHeader
          frame={frame}
          issue={issue}
          status={status}
          priority={priority}
          width={width}
          codingNow={codingNow}
        />
      )}

      {/* description + the emoji / image / attach affordances */}
      <div style={prose}>
        <div
          style={{
            marginTop: 16,
            display: "flex",
            flexDirection: "column",
            gap: 12,
          }}
        >
          {issue.descriptionParas.map((para) => (
            <p
              key={para.slice(0, 24)}
              style={{
                margin: 0,
                fontSize: 14.5,
                lineHeight: 1.55,
                color: DESC_FG,
              }}
            >
              {para}
            </p>
          ))}
        </div>

        {/* emoji / image / attach affordances under the description */}
        <div
          style={{
            marginTop: 18,
            height: 24,
            display: "flex",
            alignItems: "center",
            gap: 14,
            color: C.muted,
          }}
        >
          <IcSmile size={16} />
          <IcImage size={16} />
          <IcPaperclip size={16} />
        </div>
      </div>

      {/* full-bleed hairline */}
      <div style={{ marginTop: 12, borderTop: `1px solid ${C.strokeRow}` }} />

      {/* activity + composer (timeline.rs) */}
      <div style={{ ...prose, paddingTop: 16 }}>
        <div style={{ height: 18, fontSize: 14, fontWeight: 600, color: C.text }}>
          {`Activity (${issue.activity.length + (issue.comments?.length ?? 0)})`}
        </div>
        <div style={{ marginTop: 6, display: "flex", flexDirection: "column" }}>
          {issue.activity.map((item, i) => (
            <TimelineRow
              key={item.text}
              marker={
                <span
                  style={{
                    width: 6,
                    height: 6,
                    borderRadius: 999,
                    backgroundColor: C.muted,
                  }}
                />
              }
              markerTop={7}
              markerSize={6}
              first={i === 0}
              last={i === issue.activity.length - 1 && !issue.comments?.length}
            >
              {/* event_row: ONE muted text_xs line, the actor in the
                  foreground, ending in its relative time (EXP-723) */}
              <div
                style={{
                  height: 20,
                  display: "flex",
                  alignItems: "center",
                  fontSize: 11.5,
                  color: C.muted,
                  whiteSpace: "nowrap",
                  overflow: "hidden",
                }}
              >
                <span style={{ fontWeight: 500, color: C.text, marginRight: 3.5 }}>
                  {item.actor}
                </span>
                {item.text}
                {item.time ? ` · ${item.time}` : ""}
              </div>
            </TimelineRow>
          ))}
          {(issue.comments ?? []).map((c, i, all) => (
            <TimelineRow
              key={c.actor + c.time}
              marker={
                <span
                  style={{
                    width: 24,
                    height: 24,
                    display: "inline-flex",
                    alignItems: "center",
                    justifyContent: "center",
                    borderRadius: 999,
                    backgroundColor: "rgba(234,179,8,0.22)",
                    color: "#facc15",
                    fontSize: 10,
                    fontWeight: 600,
                  }}
                >
                  {c.initials}
                </span>
              }
              markerTop={4}
              markerSize={24}
              padY={0}
              last={i === all.length - 1}
            >
              {/* comments::comment_row — the comment is a glass CARD (radius
                  16, px_3 pt_2p5 pb_3): text_sm medium name · muted text_xs
                  time, the body, then the thread's "Leave a reply…" row
                  behind one hairline (EXP-741) */}
              <div style={{ paddingTop: 3.5, paddingBottom: 7 }}>
                <div
                  style={{
                    boxSizing: "border-box",
                    padding: "8.75px 10.5px 10.5px",
                    borderRadius: 16,
                    border: `1px solid ${C.strokeCard}`,
                    backgroundColor: C.fillCard,
                  }}
                >
                  <div
                    style={{ display: "flex", alignItems: "baseline", gap: 7 }}
                  >
                    <span style={{ fontSize: 13.5, fontWeight: 500, color: C.text }}>
                      {c.actor}
                    </span>
                    <span style={{ fontSize: 11.5, color: C.muted }}>{c.time}</span>
                  </div>
                  <p
                    style={{
                      margin: "3.5px 0 0",
                      fontSize: 13.5,
                      lineHeight: 1.5,
                      color: DESC_FG,
                    }}
                  >
                    {c.body}
                  </p>
                  <div
                    style={{
                      marginTop: 10.5,
                      paddingTop: 7,
                      borderTop: `1px solid ${C.strokeCard}`,
                    }}
                  >
                    <div style={{ padding: "3.5px 0", fontSize: 11.5, color: C.muted }}>
                      Leave a reply…
                    </div>
                  </div>
                </div>
              </div>
            </TimelineRow>
          ))}
        </div>
        {/* composer_card */}
        <div
          style={{
            marginTop: 10,
            height: 46,
            boxSizing: "border-box",
            border: `1px solid ${C.strokeStrong}`,
            borderRadius: 12,
            backgroundColor: C.fillSection,
            padding: "13px 14px",
            fontSize: 14,
            color: C.muted,
          }}
        >
          Leave a reply…
        </div>
      </div>
    </div>
  )
}
