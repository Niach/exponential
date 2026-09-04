// surfaces/detail.tsx — IssueDetailPane: the issue-detail center pane.
// EXP-471 rebuilt it against shots/issue-detail/desktop.webp — the post-EXP-282
// desktop detail has NO properties sidebar and NO rich-text toolbar:
//   · a pager row ("9 / 17" + prev/next) with copy-link · subscribe · delete right
//   · the big title
//   · ONE bordered PROPERTIES PILL BAR (status · priority · assignee · label ·
//     due · board · origin — issue_header.rs `chip_row`) with the light
//     "▷ Start coding" pill at its trailing end
//   · that launcher becomes "● Coding… / ⊗ Stop" while a LOCAL run is up
//     (coding_flow.rs); EXP-698 suppresses the synced coding-now CARD for a
//     local run, so this pane never draws one
//   · the markdown description, then the emoji / image / attach affordance row
//   · a full-bleed hairline, "Activity (n)", the event rows + comments, and the
//     "Leave a reply..." composer.
// Pixel truth: the committed store shot (1440×900 @1.25) — pane-local Ys were
// transcribed off it. All frames are composition-global; the assembler passes
// `frame` down (no useCurrentFrame here).
//
// Coordinates: the pane lays out in PANE-LOCAL px. The assembler places it at
// window-local (684, 34) — right of the expanded rail + tool window, under the
// 34px titlebar. Default size 884×917 (dock collapsed).

import React from "react"
import { interpolate, spring } from "remotion"
import { C, EASE, POP, UI_FONT, WIN } from "../theme"
import { BOARD, HERO, IDENTITY, LABELS } from "../fixtures"
import type { IssueStatus, Priority } from "../fixtures"

const CLAMP = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const
const CLAMP_EASE = { ...CLAMP, easing: EASE } as const

// Contract-sanctioned literals for this surface (matched on the ref shot).
const DESC_FG = "#d4d4d4" // body paragraph color per contract
const PRIMARY_BG = "#ededed" // the light Start coding / New Issue pill
const PRIMARY_FG = "#18181b"

// ── Layout constants (pane-local) ────────────────────────────────────────────
const DEFAULT_W = WIN.w - WIN.rail - WIN.sidebar // 884
const DEFAULT_H = WIN.h - WIN.titleBar - WIN.dockStrip // 917
// The app left-aligns the detail content at 28px. MAX_COL caps the header
// block (pager · title · properties bar) and PROSE_W the running text, so the
// description and activity never run under the phone the clips float over the
// window's right edge.
const MAX_COL = 760
const PROSE_W = 600
const PAD_X = 28

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
const IcBell: React.FC<IconProps> = (p) => (
  <Svg {...p}>
    <path d="M18 8a6 6 0 0 0-12 0c0 7-3 9-3 9h18s-3-2-3-9" />
    <path d="M13.7 21a2 2 0 0 1-3.4 0" />
  </Svg>
)
const IcTrash: React.FC<IconProps> = (p) => (
  <Svg {...p} sw={1.8}>
    <path d="M3 6h18" />
    <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6" />
    <path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
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

const IcLink: React.FC<IconProps> = (p) => (
  <Svg {...p} sw={1.8}>
    <path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" />
    <path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" />
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

const activityIconFor = (text: string): React.FC<IconProps> => {
  if (text.includes("label")) return IcTag
  if (text.includes("pull request") || text.includes("PR")) return IcGitPr
  return IcCircleDot
}

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
      padding: "0 8px",
      borderRadius: 999,
      border: `1px solid ${C.strokeCard}`,
    }}
  >
    <Icon size={13} style={{ color }} />
    <span
      style={{
        fontSize: 12,
        color: C.text,
        whiteSpace: "nowrap",
      }}
    >
      {children}
    </span>
  </div>
)

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
  activity: readonly { actor: string; text: string }[]
  comments?: readonly DetailComment[]
  imagesMeta?: string // legacy field — the current detail has no images meta row
  pr?: number
  label?: { name: string; dot: string }
  assigneeName?: string
  due?: string
  project?: string
  projectColor?: string
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
  project: IDENTITY.project,
  projectColor: IDENTITY.projectColor,
}

// ── The pane ──────────────────────────────────────────────────────────────────
export type IssueDetailPaneProps = {
  frame: number
  /** Springs the coding-now banner in; Start coding → Stop while active. */
  codingNow?: { at: number; out?: number }
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

  // Coding-now banner + Start coding → Stop swap.
  const pillPop = popIn(frame, codingNow?.at)
  const pillOut =
    codingNow?.out === undefined
      ? 1
      : interpolate(frame, [codingNow.out, codingNow.out + 8], [1, 0], CLAMP)
  const bannerT = Math.min(1, pillPop) * pillOut
  const codingActive =
    codingNow !== undefined &&
    frame >= codingNow.at &&
    (codingNow.out === undefined || frame < codingNow.out + 4)

  const st = STATUS_META[status]
  const pr = PRIO_META[priority]
  const colW = Math.min(MAX_COL, width - 2 * PAD_X)
  const col: React.CSSProperties = { marginLeft: PAD_X, width: colW }
  const prose: React.CSSProperties = {
    marginLeft: PAD_X,
    width: Math.min(PROSE_W, colW),
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
      <div style={{ ...col, paddingTop: 4 }}>
        {/* pager row: "N / M" + prev/next · copy-link · subscribe · delete */}
        <div
          style={{
            height: 20,
            display: "flex",
            alignItems: "center",
            gap: 2,
            color: C.muted,
          }}
        >
          <span style={{ fontSize: 12, marginRight: 4 }}>{issue.switcher}</span>
          <div
            style={{
              width: 18,
              height: 18,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
            }}
          >
            <IcChevronUp size={13} sw={1.8} />
          </div>
          <div
            style={{
              width: 18,
              height: 18,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
            }}
          >
            <IcChevronDown size={13} sw={1.8} />
          </div>
          <div style={{ flex: 1 }} />
          {[IcLink, IcBell, IcTrash].map((Icon, i) => (
            <div
              key={i}
              style={{
                width: 20,
                height: 20,
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
              }}
            >
              <Icon size={13} />
            </div>
          ))}
        </div>

        {/* title */}
        <div
          style={{
            marginTop: 16,
            height: 30,
            fontSize: 23,
            fontWeight: 700,
            letterSpacing: -0.3,
            lineHeight: "30px",
          }}
        >
          {issue.title}
        </div>

        {/* the properties pill bar */}
        <div
          style={{
            marginTop: 10,
            minHeight: 40,
            boxSizing: "border-box",
            display: "flex",
            alignItems: "center",
            // EXP-601: the chips WRAP inside the tray (`flex_wrap` over a
            // definite width) rather than pushing the launcher off the edge.
            flexWrap: "wrap",
            columnGap: 5,
            rowGap: 8,
            padding: "6px 7px 6px 14px",
            borderRadius: 12,
            border: `1px solid ${C.strokeCard}`,
          }}
        >
          <Prop Icon={st.Icon} color={st.color}>
            {st.label}
          </Prop>
          <Prop Icon={pr.Icon} color={pr.color}>
            {pr.label}
          </Prop>
          <Prop Icon={IcCircleUser}>{issue.assigneeName ?? "Unassigned"}</Prop>
          {issue.label ? (
            <Prop Icon={IcTag}>
              {issue.label.name.charAt(0).toUpperCase() +
                issue.label.name.slice(1)}
            </Prop>
          ) : null}
          {issue.due ? <Prop Icon={IcCalendarDays}>{issue.due}</Prop> : null}
          <Prop Icon={IcCode} color={issue.projectColor ?? "#818cf8"}>
            {issue.project ?? IDENTITY.project}
          </Prop>
          {/* EXP-496 origin chip — a widget-filed issue always carries it
              (issue_header.rs `origin_chip`), right after the Board chip. */}
          {issue.origin ? (
            <Prop Icon={IcMessageSquare}>{issue.origin}</Prop>
          ) : null}
          {/* EXP-601: the action floats on the tray's right edge (`ml_auto`),
              keeping its distance from the chips even after they wrap. */}
          <div
            style={{
              marginLeft: "auto",
              display: "flex",
              alignItems: "center",
              gap: 8,
            }}
          >
          {/* EXP-698 / coding_flow.rs: a LOCAL run turns the launcher into a
              content-sized status dot + label beside Stop — and suppresses the
              synced coding-now card, which would only double it. */}
          {codingActive ? (
            <div
              style={{
                flex: "none",
                display: "flex",
                alignItems: "center",
                gap: 6,
                fontSize: 12,
                color: C.muted,
                opacity: bannerT,
                whiteSpace: "nowrap",
              }}
            >
              <span
                style={{
                  width: 6,
                  height: 6,
                  flex: "none",
                  borderRadius: 999,
                  backgroundColor: C.green,
                }}
              />
              Coding…
            </div>
          ) : null}
          <div
            style={{
              height: 26,
              flex: "none",
              boxSizing: "border-box",
              display: "flex",
              alignItems: "center",
              gap: 6,
              padding: "0 14px",
              borderRadius: 999,
              backgroundColor: codingActive ? C.fillCard : PRIMARY_BG,
              border: codingActive ? `1px solid ${C.strokeCard}` : undefined,
              color: codingActive ? C.text : PRIMARY_FG,
              fontSize: 12.5,
              fontWeight: 600,
              whiteSpace: "nowrap",
            }}
          >
            {codingActive ? (
              <>
                <IcCircleX size={13} style={{ color: C.destructive }} />
                Stop
              </>
            ) : (
              <>
                <IcPlay size={12} sw={1.8} />
                Start coding
              </>
            )}
          </div>
          </div>
        </div>

      </div>

      {/* description + the emoji / image / attach affordances */}
      <div style={prose}>
        <div
          style={{
            marginTop: 14,
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
                fontSize: 13.5,
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

      {/* activity + composer */}
      <div style={{ ...prose, paddingTop: 16 }}>
        <div style={{ height: 18, fontSize: 13, fontWeight: 600, color: C.text }}>
          {`Activity (${issue.activity.length + (issue.comments?.length ?? 0)})`}
        </div>
        <div style={{ marginTop: 10, display: "flex", flexDirection: "column" }}>
          {issue.activity.map((item) => {
            const Icon = activityIconFor(item.text)
            return (
              <div
                key={item.text}
                style={{
                  height: 26,
                  display: "flex",
                  alignItems: "center",
                  gap: 10,
                }}
              >
                <Icon size={13} style={{ color: C.muted }} />
                <span style={{ fontSize: 12.5, color: C.muted }}>
                  <span style={{ fontWeight: 600, color: C.text }}>
                    {item.actor}
                  </span>
                  {` ${item.text}`}
                </span>
              </div>
            )
          })}
        </div>
        {(issue.comments ?? []).map((c) => (
          <div
            key={c.actor + c.time}
            style={{ marginTop: 14, display: "flex", gap: 10 }}
          >
            <span
              style={{
                width: 24,
                height: 24,
                flex: "none",
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
            <div style={{ flex: 1, minWidth: 0 }}>
              <div
                style={{ display: "flex", alignItems: "center", gap: 8, height: 16 }}
              >
                <span style={{ fontSize: 12.5, fontWeight: 600, color: C.text }}>
                  {c.actor}
                </span>
                <span style={{ fontSize: 11.5, color: C.muted }}>{c.time}</span>
              </div>
              <p
                style={{
                  margin: "5px 0 0",
                  fontSize: 13,
                  lineHeight: 1.5,
                  color: DESC_FG,
                }}
              >
                {c.body}
              </p>
            </div>
          </div>
        ))}
        {/* composer */}
        <div
          style={{
            marginTop: 18,
            height: 46,
            boxSizing: "border-box",
            border: `1px solid ${C.strokeStrong}`,
            borderRadius: 12,
            backgroundColor: C.fillSection,
            padding: "13px 14px",
            fontSize: 13,
            color: C.muted,
          }}
        >
          Leave a reply...
        </div>
      </div>
    </div>
  )
}
