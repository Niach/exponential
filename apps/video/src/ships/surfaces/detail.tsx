// surfaces/detail.tsx — IssueDetailPane: the EXP-142 center pane (Details/Changes header,
// title, markdown toolbar + description, activity, composer) + the 288px properties panel.
// Pixel truth: ref/desktop-hero-board-issue.png (right two-thirds). All frames are
// composition-global; the assembler passes `frame` down (no useCurrentFrame here).
//
// Coordinates: the pane lays out in PANE-LOCAL px. The assembler is expected to place it
// at window-local (304, 67) — right of the rail+sidebar, under the 38px top bar + 29px
// center tab strip. Default size 1264×884 (dock collapsed). See DETAIL_ANCHORS for the
// cursor-target positions of every clickable element (pane-local).

import React from "react"
import { interpolate, interpolateColors, spring } from "remotion"
import { C, EASE, MONO_FONT, POP, UI_FONT, WIN } from "../theme"
import { BOARD, HERO, IDENTITY, LABELS, RELEASE } from "../fixtures"
import type { IssueStatus, Priority } from "../fixtures"
import { riseIn } from "../rig"

const CLAMP = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const
const CLAMP_EASE = { ...CLAMP, easing: EASE } as const

// Contract-sanctioned literals for this surface (matched on the ref shot).
const DESC_FG = "#d4d4d4" // body paragraph color per contract
const GREEN_BORDER = "rgba(34,197,94,0.4)" // coding-now pill border

// ── Layout constants (pane-local) ────────────────────────────────────────────
const HEADER_H = 34
const PAD_X = 16
const PROPS_W = WIN.propsPanel // 288
const DEFAULT_W = WIN.w - WIN.rail - WIN.sidebar // 1264
const DEFAULT_H = WIN.h - WIN.topBar - WIN.dockTabs - WIN.dockStrip // 884
const COL_W = 768 // centered content column (incl. its 16px side padding)
const BTN_START_W = 104
const BTN_STOP_W = 60
const BTN_SUB_W = 92
const PILL_W = 184
const PR_CHIP_W = 94

// Pane-local anchor points for the cursor rig (details state, defaults, no pill/chip).
// Window-local = anchor + (304, 67) when the pane sits under the center tab strip.
export const DETAIL_ANCHORS = {
  detailsTab: { x: 40, y: 17 },
  changesTab: { x: 110, y: 17 },
  switcher: { x: DEFAULT_W - 305, y: 17 },
  prevIssue: { x: DEFAULT_W - 267, y: 17 },
  nextIssue: { x: DEFAULT_W - 245, y: 17 },
  startCoding: { x: DEFAULT_W - 172, y: 17 },
  subscribed: { x: DEFAULT_W - 62, y: 17 },
  title: { x: 224, y: 70 },
  composerInput: { x: 300, y: 428 },
  composerSend: { x: 843, y: 444 },
  propsStatus: { x: 1020, y: 85 },
  propsPriority: { x: 1020, y: 149 },
  propsLabels: { x: 1020, y: 213 },
  propsRelease: { x: 1020, y: 277 },
  propsDueDate: { x: 1020, y: 341 },
  propsRecurrence: { x: 1030, y: 367 },
  propsProject: { x: 1030, y: 431 },
} as const

// ── Tiny inline icons (lucide-like, stroke currentColor) ─────────────────────
type IconProps = { size?: number; sw?: number; style?: React.CSSProperties }
const Svg: React.FC<IconProps & { children: React.ReactNode }> = ({ size = 14, sw = 2, style, children }) => (
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
const IcBell: React.FC<IconProps> = (p) => (
  <Svg {...p}>
    <path d="M18 8a6 6 0 0 0-12 0c0 7-3 9-3 9h18s-3-2-3-9" />
    <path d="M13.7 21a2 2 0 0 1-3.4 0" />
  </Svg>
)
const IcChevronUp: React.FC<IconProps> = (p) => (
  <Svg {...p}>
    <path d="m6 15 6-6 6 6" />
  </Svg>
)
const IcChevronDown: React.FC<IconProps> = (p) => (
  <Svg {...p}>
    <path d="m6 9 6 6 6-6" />
  </Svg>
)
const IcTag: React.FC<IconProps> = (p) => (
  <Svg {...p}>
    <path d="M12.586 2.586A2 2 0 0 0 11.172 2H4a2 2 0 0 0-2 2v7.172a2 2 0 0 0 .586 1.414l8.704 8.704a2.426 2.426 0 0 0 3.42 0l6.58-6.58a2.426 2.426 0 0 0 0-3.42z" />
    <circle cx="7.5" cy="7.5" r="0.6" />
  </Svg>
)
const IcRocket: React.FC<IconProps> = (p) => (
  <Svg {...p}>
    <path d="M4.5 16.5c-1.5 1.26-2 5-2 5s3.74-.5 5-2c.71-.84.7-2.13-.09-2.91a2.18 2.18 0 0 0-2.91-.09z" />
    <path d="m12 15-3-3a22 22 0 0 1 2-3.95A12.88 12.88 0 0 1 22 2c0 2.72-.78 7.5-6 11a22.35 22.35 0 0 1-4 2z" />
    <path d="M9 12H4s.55-3.03 2-4c1.62-1.08 5 0 5 0" />
    <path d="M12 15v5s3.03-.55 4-2c1.08-1.62 0-5 0-5" />
  </Svg>
)
const IcCalendarDays: React.FC<IconProps> = (p) => (
  <Svg {...p}>
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
const IcRepeat: React.FC<IconProps> = (p) => (
  <Svg {...p}>
    <path d="m17 2 4 4-4 4" />
    <path d="M3 11v-1a4 4 0 0 1 4-4h14" />
    <path d="m7 22-4-4 4-4" />
    <path d="M21 13v1a4 4 0 0 1-4 4H3" />
  </Svg>
)
const IcSend: React.FC<IconProps> = (p) => (
  <Svg {...p}>
    <path d="M14.536 21.686a.5.5 0 0 0 .937-.024l6.5-19a.496.496 0 0 0-.635-.635l-19 6.5a.5.5 0 0 0-.024.937l7.93 3.18a2 2 0 0 1 1.112 1.11z" />
    <path d="m21.854 2.147-10.94 10.939" />
  </Svg>
)
const IcGitPr: React.FC<IconProps> = (p) => (
  <Svg {...p}>
    <circle cx="6" cy="6" r="3" />
    <circle cx="18" cy="18" r="3" />
    <path d="M13 6h3a2 2 0 0 1 2 2v7" />
    <path d="M6 9v12" />
  </Svg>
)
const IcCircleDot: React.FC<IconProps> = (p) => (
  <Svg {...p}>
    <circle cx="12" cy="12" r="9" />
    <circle cx="12" cy="12" r="1.4" fill="currentColor" stroke="none" />
  </Svg>
)
// Markdown toolbar glyphs
const IcCode: React.FC<IconProps> = (p) => (
  <Svg {...p}>
    <path d="m16 18 6-6-6-6" />
    <path d="m8 6-6 6 6 6" />
  </Svg>
)
const IcLink: React.FC<IconProps> = (p) => (
  <Svg {...p}>
    <path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" />
    <path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" />
  </Svg>
)
const IcQuote: React.FC<IconProps> = (p) => (
  <Svg {...p} sw={1.7}>
    <path d="M16 3a2 2 0 0 0-2 2v6a2 2 0 0 0 2 2 1 1 0 0 1 1 1v1a2 2 0 0 1-2 2 1 1 0 0 0-1 1v2a1 1 0 0 0 1 1 6 6 0 0 0 6-6V5a2 2 0 0 0-2-2z" />
    <path d="M5 3a2 2 0 0 0-2 2v6a2 2 0 0 0 2 2 1 1 0 0 1 1 1v1a2 2 0 0 1-2 2 1 1 0 0 0-1 1v2a1 1 0 0 0 1 1 6 6 0 0 0 6-6V5a2 2 0 0 0-2-2z" />
  </Svg>
)
const IcList: React.FC<IconProps> = (p) => (
  <Svg {...p}>
    <path d="M3 6h.01" />
    <path d="M8 6h13" />
    <path d="M3 12h.01" />
    <path d="M8 12h13" />
    <path d="M3 18h.01" />
    <path d="M8 18h13" />
  </Svg>
)
const IcListOrdered: React.FC<IconProps> = (p) => (
  <Svg {...p} sw={1.7}>
    <path d="M10 6h11" />
    <path d="M10 12h11" />
    <path d="M10 18h11" />
    <path d="M4 6h1v4" />
    <path d="M4 10h2" />
    <path d="M6 18H4c0-1 2-2 2-3s-1-1.5-2-1" />
  </Svg>
)
const IcListChecks: React.FC<IconProps> = (p) => (
  <Svg {...p}>
    <path d="m3 7 2 2 4-4" />
    <path d="m3 17 2 2 4-4" />
    <path d="M13 6h8" />
    <path d="M13 12h8" />
    <path d="M13 18h8" />
  </Svg>
)
const IcImage: React.FC<IconProps> = (p) => (
  <Svg {...p} sw={1.7}>
    <rect x="3" y="3" width="18" height="18" rx="2" />
    <circle cx="9" cy="9" r="2" />
    <path d="m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21" />
  </Svg>
)
// Priority: signal-high (three ascending bars + baseline dot)
const IcSignalHigh: React.FC<IconProps> = (p) => (
  <Svg {...p}>
    <path d="M4 20h.01" />
    <path d="M8.5 20v-5" />
    <path d="M13 20v-9" />
    <path d="M17.5 20V6" />
  </Svg>
)
const IcMinus: React.FC<IconProps> = (p) => (
  <Svg {...p}>
    <path d="M5 12h14" />
  </Svg>
)
// Status glyphs
const IcCircle: React.FC<IconProps> = (p) => (
  <Svg {...p}>
    <circle cx="12" cy="12" r="9" />
  </Svg>
)
const IcCircleDashed: React.FC<IconProps> = (p) => (
  <Svg {...p}>
    <circle cx="12" cy="12" r="9" strokeDasharray="3.6 3.4" />
  </Svg>
)
const IcTimer: React.FC<IconProps> = (p) => (
  <Svg {...p}>
    <path d="M10 2h4" />
    <path d="m12 14 3-3" />
    <circle cx="12" cy="14" r="8" />
  </Svg>
)
const IcCircleCheck: React.FC<IconProps> = (p) => (
  <Svg {...p}>
    <circle cx="12" cy="12" r="9" />
    <path d="m8.5 12 2.5 2.5 5-5" />
  </Svg>
)

const STATUS_META: Record<IssueStatus, { label: string; color: string; Icon: React.FC<IconProps> }> = {
  backlog: { label: "Backlog", color: C.statusBacklog, Icon: IcCircleDashed },
  todo: { label: "Todo", color: C.statusTodo, Icon: IcCircle },
  in_progress: { label: "In Progress", color: C.statusInProgress, Icon: IcTimer },
  done: { label: "Done", color: C.statusDone, Icon: IcCircleCheck },
}
const PRIO_META: Record<Priority, { label: string; color: string; Icon: React.FC<IconProps> }> = {
  none: { label: "No priority", color: C.muted, Icon: IcMinus },
  urgent: { label: "Urgent", color: C.prioUrgent, Icon: IcSignalHigh },
  high: { label: "High", color: C.prioHigh, Icon: IcSignalHigh },
  medium: { label: "Medium", color: C.prioMedium, Icon: IcSignalHigh },
  low: { label: "Low", color: C.prioLow, Icon: IcSignalHigh },
}

// ── Markdown toolbar ─────────────────────────────────────────────────────────
const LetterGlyph: React.FC<{ main: string; sub?: string; italic?: boolean; strike?: boolean }> = ({
  main,
  sub,
  italic,
  strike,
}) => (
  <span
    style={{
      fontFamily: UI_FONT,
      fontSize: 12.5,
      fontWeight: 600,
      fontStyle: italic ? "italic" : undefined,
      textDecoration: strike ? "line-through" : undefined,
      lineHeight: 1,
      display: "flex",
      alignItems: "baseline",
    }}
  >
    {main}
    {sub === undefined ? null : <span style={{ fontSize: 8.5, fontWeight: 600, translate: "0px 1px" }}>{sub}</span>}
  </span>
)

const TOOLBAR_GROUPS: React.ReactNode[][] = [
  [<LetterGlyph key="h1" main="H" sub="1" />, <LetterGlyph key="h2" main="H" sub="2" />, <LetterGlyph key="h3" main="H" sub="3" />],
  [
    <LetterGlyph key="b" main="B" />,
    <LetterGlyph key="i" main="I" italic />,
    <LetterGlyph key="s" main="S" strike />,
    <IcCode key="c" size={14} />,
  ],
  [<IcLink key="l" size={13} />, <IcQuote key="q" size={13} />],
  [<IcList key="ul" size={14} />, <IcListOrdered key="ol" size={14} />, <IcListChecks key="tl" size={14} />],
  [<LetterGlyph key="tx" main="T" sub="x" />],
  [<IcImage key="img" size={14} />],
]

const MarkdownToolbar: React.FC = () => (
  <div
    style={{
      height: 34,
      display: "flex",
      alignItems: "center",
      padding: "0 8px",
      borderBottom: `1px solid ${C.borderSoft}`,
      color: C.muted,
    }}
  >
    {TOOLBAR_GROUPS.map((group, gi) => (
      <React.Fragment key={gi}>
        {gi > 0 ? <div style={{ width: 1, height: 16, backgroundColor: C.border, margin: "0 5px" }} /> : null}
        {group.map((glyph, bi) => (
          <div
            key={bi}
            style={{ width: 24, height: 24, display: "flex", alignItems: "center", justifyContent: "center", borderRadius: 5 }}
          >
            {glyph}
          </div>
        ))}
      </React.Fragment>
    ))}
  </div>
)

// ── Small shared bits ─────────────────────────────────────────────────────────
const popIn = (frame: number, at: number | undefined) =>
  at === undefined || frame < at ? 0 : spring({ frame: frame - at, fps: 30, config: POP })

const LabelPill: React.FC<{ name: string; dot: string }> = ({ name, dot }) => (
  <div
    style={{
      display: "inline-flex",
      alignItems: "center",
      gap: 6,
      height: 20,
      padding: "0 8px",
      borderRadius: 999,
      border: `1px solid ${C.border}`,
    }}
  >
    <div style={{ width: 6, height: 6, borderRadius: 999, backgroundColor: dot }} />
    <span style={{ fontSize: 12, color: C.muted }}>{name}</span>
  </div>
)

const activityIconFor = (text: string): React.FC<IconProps> => {
  if (text.includes("label")) return IcTag
  if (text.includes("release")) return IcRocket
  if (text.includes("pull request")) return IcGitPr
  return IcCircleDot
}

// ── Properties panel ─────────────────────────────────────────────────────────
const PropGroup: React.FC<{
  label: string
  frame: number
  staggerAt?: number
  index: number
  children: React.ReactNode
}> = ({ label, frame, staggerAt, index, children }) => {
  const anim = staggerAt === undefined ? { opacity: 1, translate: "0px 0px" } : riseIn(frame, staggerAt + index * 4, 9, 8)
  return (
    <div style={{ display: "flex", flexDirection: "column", ...anim }}>
      <div
        style={{
          height: 16,
          fontSize: 11,
          fontWeight: 500,
          letterSpacing: 0.5,
          textTransform: "uppercase",
          color: C.muted,
          marginBottom: 6,
          display: "flex",
          alignItems: "center",
        }}
      >
        {label}
      </div>
      {children}
    </div>
  )
}

const PropValue: React.FC<{ icon: React.ReactNode; children: React.ReactNode; muted?: boolean }> = ({
  icon,
  children,
  muted,
}) => (
  <div style={{ height: 22, display: "flex", alignItems: "center", gap: 8 }}>
    {icon}
    <span style={{ fontSize: 13, color: muted ? C.muted : C.text }}>{children}</span>
  </div>
)

// Issue content shown by the pane. Everything defaults to the ships HERO
// fixture so existing callers render exactly as before.
export type DetailIssueContent = {
  id: string
  title: string
  descriptionParas: readonly string[]
  switcher: string
  activity: readonly { actor: string; text: string }[]
  imagesMeta?: string // "0 images" meta row text
  pr?: number // default PR-chip label number
  label?: { name: string; dot: string }
  due?: string
  release?: string
  project?: string
  projectColor?: string
}

const HERO_ISSUE: DetailIssueContent = {
  id: HERO.id,
  title: HERO.title,
  descriptionParas: HERO.descriptionParas,
  switcher: HERO.switcher,
  activity: HERO.activity,
  imagesMeta: "0 images",
  pr: HERO.pr,
  label: LABELS.bug,
  due: BOARD.find((r) => r.id === HERO.id)?.due ?? "Jul 15",
  release: RELEASE.name,
  project: IDENTITY.project,
  projectColor: IDENTITY.projectColor,
}

const PropsPanel: React.FC<{
  frame: number
  staggerAt?: number
  status: IssueStatus
  priority: Priority
  issue: DetailIssueContent
  showRelease: boolean
}> = ({ frame, staggerAt, status, priority, issue, showRelease }) => {
  const st = STATUS_META[status]
  const pr = PRIO_META[priority]
  const due = issue.due
  const label = issue.label
  // Keep the stagger rhythm stable regardless of which optional groups render.
  let index = 1
  const nextIndex = () => {
    index += 1
    return index
  }
  return (
    <div
      style={{
        width: PROPS_W,
        flexShrink: 0,
        borderLeft: `1px solid ${C.border}`,
        padding: "18px 12px",
        display: "flex",
        flexDirection: "column",
        gap: 20,
      }}
    >
      <PropGroup label="Status" frame={frame} staggerAt={staggerAt} index={0}>
        <PropValue icon={<st.Icon size={14} style={{ color: st.color }} />}>{st.label}</PropValue>
      </PropGroup>
      <PropGroup label="Priority" frame={frame} staggerAt={staggerAt} index={1}>
        <PropValue icon={<pr.Icon size={14} style={{ color: pr.color }} />}>{pr.label}</PropValue>
      </PropGroup>
      {label !== undefined ? (
        <PropGroup label="Labels" frame={frame} staggerAt={staggerAt} index={nextIndex()}>
          <div style={{ height: 22, display: "flex", alignItems: "center" }}>
            <LabelPill name={label.name} dot={label.dot} />
          </div>
        </PropGroup>
      ) : null}
      {showRelease ? (
        <PropGroup label="Release" frame={frame} staggerAt={staggerAt} index={nextIndex()}>
          <PropValue icon={<IcRocket size={14} style={{ color: C.muted }} />}>{issue.release ?? RELEASE.name}</PropValue>
        </PropGroup>
      ) : null}
      <PropGroup label="Due date" frame={frame} staggerAt={staggerAt} index={nextIndex()}>
        {due !== undefined ? (
          <PropValue icon={<IcCalendarDays size={14} style={{ color: C.muted }} />}>{due}</PropValue>
        ) : (
          <PropValue icon={<IcCalendarDays size={14} style={{ color: C.muted }} />} muted>
            Add due date
          </PropValue>
        )}
        <div style={{ height: 22, marginTop: 4, display: "flex", alignItems: "center", gap: 8 }}>
          <IcRepeat size={13} style={{ color: C.muted }} />
          <span style={{ fontSize: 12.5, color: C.muted }}>Add recurrence</span>
        </div>
      </PropGroup>
      <PropGroup label="Project" frame={frame} staggerAt={staggerAt} index={nextIndex()}>
        <div style={{ height: 22, display: "flex", alignItems: "center" }}>
          <div
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: 8,
              height: 24,
              padding: "0 10px",
              borderRadius: 6,
              backgroundColor: C.accentBg,
            }}
          >
            <div style={{ width: 8, height: 8, borderRadius: 999, backgroundColor: issue.projectColor ?? IDENTITY.projectColor }} />
            <span style={{ fontSize: 12, color: C.text }}>{issue.project ?? IDENTITY.project}</span>
          </div>
        </div>
      </PropGroup>
    </div>
  )
}

// ── The pane ──────────────────────────────────────────────────────────────────
export type DetailTab = "details" | "changes"

export type IssueDetailPaneProps = {
  frame: number
  /** Target tab; with tabSwitchAt set, the previous tab shows before that frame. */
  tab?: DetailTab
  /** Global frame the Details↔Changes switch happens (8f color/body crossfade). */
  tabSwitchAt?: number
  /** Springs the "Coding now · MacBook Pro" pill; Start coding → Stop while active. */
  codingNow?: { at: number; out?: number }
  /** Pops a PR chip into the header right cluster (default label from fixtures). */
  prChip?: { at: number; label?: string }
  /** Properties panel groups stagger-fade in (4f stagger, 8px rise). */
  staggerAt?: number
  /** Whole pane slides in from the right 46px + fades over 20f (S4 entrance). */
  slideInAt?: number
  /** Green live dot pops next to the Changes tab label. */
  changesLiveAt?: number
  /** Hover highlight window on the Start coding button (cursor choreography). */
  startHover?: { at: number; out?: number }
  /** Properties STATUS value (board truth changes over the film). */
  status?: IssueStatus
  priority?: Priority
  subscribed?: boolean
  /** Issue content (title/description/activity/properties). Default: the ships HERO. */
  issue?: DetailIssueContent
  /** Render the RELEASE properties group (default true — the ships film shows it). */
  showRelease?: boolean
  width?: number
  height?: number
}

export const IssueDetailPane: React.FC<IssueDetailPaneProps> = ({
  frame,
  tab = "details",
  tabSwitchAt,
  codingNow,
  prChip,
  staggerAt,
  slideInAt,
  changesLiveAt,
  startHover,
  status = "todo",
  priority = "high",
  subscribed = true,
  issue = HERO_ISSUE,
  showRelease = true,
  width = DEFAULT_W,
  height = DEFAULT_H,
}) => {
  // Tab crossfade: t=1 means the target `tab` state fully applies.
  const switchT =
    tabSwitchAt === undefined ? 1 : interpolate(frame, [tabSwitchAt, tabSwitchAt + 8], [0, 1], CLAMP_EASE)
  const detailsActive = tab === "details" ? switchT : 1 - switchT
  const detailsColor = interpolateColors(detailsActive, [0, 1], [C.muted, C.text])
  const changesColor = interpolateColors(detailsActive, [0, 1], [C.text, C.muted])
  const detailsBodyO = detailsActive

  // Entrance slide (S4).
  const slide =
    slideInAt === undefined
      ? { opacity: 1, translate: "0px 0px" }
      : {
          opacity: interpolate(frame, [slideInAt, slideInAt + 20], [0, 1], CLAMP_EASE),
          translate: `${interpolate(frame, [slideInAt, slideInAt + 20], [46, 0], CLAMP_EASE)}px 0px`,
        }

  // Coding-now pill + Start coding → Stop swap.
  const pillPop = popIn(frame, codingNow?.at)
  const pillOut =
    codingNow?.out === undefined ? 1 : interpolate(frame, [codingNow.out, codingNow.out + 8], [1, 0], CLAMP)
  const pillW = PILL_W * Math.min(1, pillPop) * pillOut
  const pillO = Math.min(1, pillPop * 1.5) * pillOut
  const codingActive =
    codingNow !== undefined && frame >= codingNow.at && (codingNow.out === undefined || frame < codingNow.out + 4)

  // PR chip pop.
  const chipPop = popIn(frame, prChip?.at)
  const chipW = PR_CHIP_W * Math.min(1, chipPop)

  // Changes live dot.
  const liveDot = popIn(frame, changesLiveAt)

  // Start-coding hover highlight.
  const hoverIn = startHover === undefined ? 0 : interpolate(frame, [startHover.at, startHover.at + 6], [0, 1], CLAMP)
  const hoverOut =
    startHover?.out === undefined ? 1 : interpolate(frame, [startHover.out, startHover.out + 6], [1, 0], CLAMP)
  const hoverT = hoverIn * hoverOut

  const leftColW = width - PROPS_W
  const colMargin = Math.max(0, (leftColW - COL_W) / 2)

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
      {/* ── Header row ── */}
      <div
        style={{
          height: HEADER_H,
          display: "flex",
          alignItems: "center",
          padding: `0 ${PAD_X}px`,
          borderBottom: `1px solid ${C.border}`,
          gap: 12,
        }}
      >
        <span style={{ fontSize: 13, fontWeight: 500, color: detailsColor }}>Details</span>
        <span style={{ fontSize: 13, fontWeight: 500, color: changesColor, display: "flex", alignItems: "center", gap: 6, marginLeft: 8 }}>
          Changes
          {changesLiveAt !== undefined && frame >= changesLiveAt ? (
            <span
              style={{
                width: 6,
                height: 6,
                borderRadius: 999,
                backgroundColor: C.green,
                scale: String(liveDot),
              }}
            />
          ) : null}
        </span>
        <div style={{ flex: 1 }} />
        {/* switcher */}
        <span style={{ fontSize: 12.5, color: C.muted }}>{issue.switcher}</span>
        <div style={{ display: "flex", alignItems: "center", gap: 4 }}>
          <div style={{ width: 18, height: 18, display: "flex", alignItems: "center", justifyContent: "center", color: C.muted }}>
            <IcChevronUp size={13} />
          </div>
          <div style={{ width: 18, height: 18, display: "flex", alignItems: "center", justifyContent: "center", color: C.muted }}>
            <IcChevronDown size={13} />
          </div>
        </div>
        {/* PR chip (pops after the PR opens) */}
        {prChip !== undefined && frame >= prChip.at ? (
          <div
            style={{
              width: chipW,
              overflow: "hidden",
              display: "flex",
              justifyContent: "flex-end",
              opacity: Math.min(1, chipPop * 1.5),
            }}
          >
            <div
              style={{
                display: "inline-flex",
                alignItems: "center",
                gap: 6,
                height: 22,
                padding: "0 9px",
                borderRadius: 999,
                border: `1px solid ${C.border}`,
                scale: String(chipPop),
                flexShrink: 0,
              }}
            >
              <IcGitPr size={12} style={{ color: C.green }} />
              <span style={{ fontFamily: MONO_FONT, fontSize: 11, color: C.text, whiteSpace: "nowrap" }}>
                {prChip.label ?? `PR #${issue.pr ?? HERO.pr}`}
              </span>
            </div>
          </div>
        ) : null}
        {/* Coding-now pill */}
        {codingNow !== undefined && frame >= codingNow.at ? (
          <div style={{ width: pillW, overflow: "hidden", display: "flex", justifyContent: "flex-end", opacity: pillO }}>
            <div
              style={{
                display: "inline-flex",
                alignItems: "center",
                gap: 7,
                height: 22,
                padding: "0 10px",
                borderRadius: 999,
                border: `1px solid ${GREEN_BORDER}`,
                scale: String(Math.max(0, pillPop) * pillOut),
                flexShrink: 0,
              }}
            >
              <span style={{ width: 6, height: 6, borderRadius: 999, backgroundColor: C.green }} />
              <span style={{ fontSize: 12, color: C.text, whiteSpace: "nowrap" }}>{`Coding now · ${IDENTITY.device}`}</span>
            </div>
          </div>
        ) : null}
        {/* Start coding / Stop */}
        <div
          style={{
            width: codingActive ? BTN_STOP_W : BTN_START_W,
            height: 24,
            borderRadius: 6,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            gap: 6,
            backgroundColor: hoverT > 0 ? `rgba(255,255,255,${0.08 * hoverT})` : undefined,
          }}
        >
          {codingActive ? (
            <>
              <IcCircleX size={13} style={{ color: C.destructive }} />
              <span style={{ fontSize: 12.5, fontWeight: 500, color: C.text }}>Stop</span>
            </>
          ) : (
            <>
              <IcPlay size={13} sw={1.8} style={{ color: C.green, opacity: 0.85 + 0.15 * hoverT }} />
              <span style={{ fontSize: 12.5, fontWeight: 500, color: C.text, whiteSpace: "nowrap" }}>Start coding</span>
            </>
          )}
        </div>
        {/* Subscribe toggle */}
        <div
          style={{
            width: BTN_SUB_W,
            height: 24,
            borderRadius: 6,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            gap: 6,
          }}
        >
          <IcBell size={13} style={{ color: C.muted }} />
          <span style={{ fontSize: 12.5, fontWeight: 500, color: C.text }}>{subscribed ? "Subscribed" : "Subscribe"}</span>
        </div>
      </div>

      {/* ── Body (Details tab): left column + properties panel ── */}
      {detailsBodyO > 0.01 ? (
        <div
          style={{
            position: "absolute",
            left: 0,
            top: HEADER_H,
            width,
            height: height - HEADER_H,
            display: "flex",
            opacity: detailsBodyO,
          }}
        >
          {/* left column */}
          <div style={{ width: leftColW, flexShrink: 0, overflow: "hidden" }}>
            {/* centered content column */}
            <div style={{ marginLeft: colMargin, width: COL_W, padding: `0 ${PAD_X}px` }}>
              <div style={{ paddingTop: 22, height: 28, fontSize: 20, fontWeight: 600, letterSpacing: -0.2, lineHeight: "28px", boxSizing: "content-box" }}>
                {issue.title}
              </div>
              {/* editor box: toolbar + description */}
              <div style={{ marginTop: 12, border: `1px solid ${C.border}`, borderRadius: 6, height: 166, overflow: "hidden" }}>
                <MarkdownToolbar />
                <div style={{ padding: "14px 16px", display: "flex", flexDirection: "column", gap: 8 }}>
                  {issue.descriptionParas.map((para) => (
                    <p key={para.slice(0, 24)} style={{ margin: 0, fontSize: 13.5, lineHeight: 1.6, color: DESC_FG }}>
                      {para}
                    </p>
                  ))}
                </div>
              </div>
              <div style={{ marginTop: 8, height: 16, fontSize: 12, color: C.muted, textAlign: "right" }}>{issue.imagesMeta ?? "0 images"}</div>
            </div>
            {/* full-bleed divider */}
            <div style={{ marginTop: 14, borderTop: `1px solid ${C.border}` }} />
            {/* activity + composer (re-centered) */}
            <div style={{ marginLeft: colMargin, width: COL_W, padding: `14px ${PAD_X}px 0` }}>
              <div style={{ height: 16, fontSize: 12, fontWeight: 500, color: C.muted }}>
                {`Activity (${issue.activity.length})`}
              </div>
              <div style={{ marginTop: 8, display: "flex", flexDirection: "column" }}>
                {issue.activity.map((item) => {
                  const Icon = activityIconFor(item.text)
                  return (
                    <div key={item.text} style={{ height: 24, display: "flex", alignItems: "center", gap: 9 }}>
                      <Icon size={13} style={{ color: C.muted }} />
                      <span style={{ fontSize: 12.5, color: C.muted }}>
                        <span style={{ fontWeight: 600, color: C.text }}>{item.actor}</span>
                        {` ${item.text}`}
                      </span>
                    </div>
                  )
                })}
              </div>
              {/* composer */}
              <div style={{ marginTop: 12, display: "flex", alignItems: "flex-end", gap: 8 }}>
                <div
                  style={{
                    flex: 1,
                    height: 58,
                    border: `1px solid ${C.input}`,
                    borderRadius: 8,
                    padding: "10px 12px",
                    fontSize: 13,
                    color: C.muted,
                  }}
                >
                  Leave a reply...
                </div>
                <div
                  style={{
                    width: 26,
                    height: 26,
                    borderRadius: 6,
                    backgroundColor: C.accentBg,
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "center",
                    color: C.muted,
                  }}
                >
                  <IcSend size={13} sw={1.7} />
                </div>
              </div>
            </div>
          </div>
          {/* properties panel */}
          <PropsPanel frame={frame} staggerAt={staggerAt} status={status} priority={priority} issue={issue} showRelease={showRelease} />
        </div>
      ) : null}
    </div>
  )
}
