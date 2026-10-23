// surfaces/board.tsx — issue-board primitives (status/priority/avatar/label/
// calendar glyphs). The board LIST itself is boardlist.tsx (the main view of
// the cutout panel since the sidebar stopped listing a board's issues).
// Done is BLUE (contract issueStatusDefaults); In Review is the green
// progress-3-4 glyph (icons.json status-in-review).

import React from "react"
import { C, UI_FONT } from "../theme"
import { type IssueStatus, type Priority } from "../fixtures"
import { StatusGlyph } from "./shell"

// Avatar recipe sampled from the desktop-hero-board-issue reference screenshot (the DS circles):
// dark fuchsia fill ≈ #3d0f3a, brighter fuchsia ring, bright fuchsia initials.
// (Deliberately local — the shared theme has no fuchsia token; matched to the ref.)
// The app tints each member's circle from their identity; the reference board
// shows blue / amber / green / emerald side by side, so the film derives the
// accent from the initials instead of painting everyone fuchsia.
const AVATAR_ACCENTS = [
  { bg: `rgba(59,130,246,0.30)`, fg: `#93c5fd` },
  { bg: `rgba(245,158,11,0.30)`, fg: `#fcd34d` },
  { bg: `rgba(34,197,94,0.30)`, fg: `#86efac` },
  { bg: `rgba(217,70,239,0.28)`, fg: `#e879f9` },
  { bg: `rgba(20,184,166,0.30)`, fg: `#5eead4` },
] as const

const avatarAccent = (initials: string) => {
  let h = 0
  for (const ch of initials) h = (h * 31 + ch.charCodeAt(0)) % 997
  return AVATAR_ACCENTS[h % AVATAR_ACCENTS.length]
}

// ── Tiny inline icons (lucide-style, stroke currentColor) ────────────────────
const svgProps = (size: number, strokeWidth = 1.6) =>
  ({
    width: size,
    height: size,
    viewBox: `0 0 24 24`,
    fill: `none`,
    stroke: `currentColor`,
    strokeWidth,
    strokeLinecap: `round`,
    strokeLinejoin: `round`,
  }) as const

const CircleDashedIcon: React.FC<{ size?: number }> = ({ size = 13 }) => (
  <svg {...svgProps(size, 2)}>
    <circle cx="12" cy="12" r="9" strokeDasharray="3.6 3.4" />
  </svg>
)

// Pie-clock started glyph (icons.json custom progress-2-4 — the builtin
// In Progress icon since EXP-314): ring + half wedge.
const PieClockIcon: React.FC<{ size?: number }> = ({ size = 13 }) => (
  <svg {...svgProps(size, 2)}>
    <circle cx="12" cy="12" r="10" />
    <path
      d="M12 12 L12 6 A6 6 0 0 1 12 18 Z"
      fill="currentColor"
      stroke="none"
    />
  </svg>
)

const CircleCheckIcon: React.FC<{ size?: number }> = ({ size = 13 }) => (
  <svg {...svgProps(size, 2)}>
    <circle cx="12" cy="12" r="9" />
    <path d="m9 12 2 2 4-4" />
  </svg>
)

const MinusIcon: React.FC<{ size?: number }> = ({ size = 13 }) => (
  <svg {...svgProps(size, 2)}>
    <path d="M5 12h14" />
  </svg>
)

const TriangleAlertIcon: React.FC<{ size?: number }> = ({ size = 13 }) => (
  <svg {...svgProps(size, 2)}>
    <path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 20h16a2 2 0 0 0 1.73-2Z" />
    <path d="M12 9v4" />
    <path d="M12 17h.01" />
  </svg>
)

// lucide signal-low / signal-medium / signal-high: baseline dot + 1/2/3 ascending bars.
const SignalIcon: React.FC<{ bars: 1 | 2 | 3; size?: number }> = ({
  bars,
  size = 13,
}) => (
  <svg {...svgProps(size, 2)}>
    <path d="M2 20h.01" />
    <path d="M7 20v-4" />
    {bars >= 2 ? <path d="M12 20v-8" /> : null}
    {bars >= 3 ? <path d="M17 20V8" /> : null}
  </svg>
)

const UserIcon: React.FC<{ size?: number }> = ({ size = 10 }) => (
  <svg {...svgProps(size, 2)}>
    <path d="M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2" />
    <circle cx="12" cy="7" r="4" />
  </svg>
)

// Exported per contract — the due-date calendar-days glyph on board rows.
export const CalendarGlyph: React.FC<{ size?: number }> = ({ size = 13 }) => (
  <svg {...svgProps(size, 1.6)}>
    <path d="M8 2v4" />
    <path d="M16 2v4" />
    <rect x="3" y="4" width="18" height="18" rx="2" />
    <path d="M3 10h18" />
    <path d="M8 14h.01" />
    <path d="M12 14h.01" />
    <path d="M16 14h.01" />
    <path d="M8 18h.01" />
    <path d="M12 18h.01" />
  </svg>
)

// ── Primitives ────────────────────────────────────────────────────────────────
export const StatusIcon: React.FC<{ status: IssueStatus; size?: number }> = ({
  status,
  size = 13,
}) => {
  switch (status) {
    case `backlog`:
      return (
        <span style={{ color: C.statusBacklog, display: `flex` }}>
          <CircleDashedIcon size={size} />
        </span>
      )
    case `in_progress`:
      return (
        <span style={{ color: C.statusInProgress, display: `flex` }}>
          <PieClockIcon size={size} />
        </span>
      )
    case `in_review`:
      return (
        <span style={{ color: C.statusInReview, display: `flex` }}>
          <StatusGlyph status="in_review" size={size} />
        </span>
      )
    case `done`:
      return (
        <span style={{ color: C.statusDone, display: `flex` }}>
          <CircleCheckIcon size={size} />
        </span>
      )
  }
}

export const PriorityIcon: React.FC<{ p: Priority; size?: number }> = ({
  p,
  size = 13,
}) => {
  switch (p) {
    case `none`:
      return (
        <span style={{ color: C.muted, display: `flex`, opacity: 0.8 }}>
          <MinusIcon size={size} />
        </span>
      )
    case `urgent`:
      return (
        <span style={{ color: C.prioUrgent, display: `flex` }}>
          <TriangleAlertIcon size={size} />
        </span>
      )
    case `high`:
      return (
        <span style={{ color: C.prioHigh, display: `flex` }}>
          <SignalIcon bars={3} size={size} />
        </span>
      )
    case `medium`:
      return (
        <span style={{ color: C.prioMedium, display: `flex` }}>
          <SignalIcon bars={2} size={size} />
        </span>
      )
    case `low`:
      return (
        <span style={{ color: C.prioLow, display: `flex` }}>
          <SignalIcon bars={1} size={size} />
        </span>
      )
  }
}

// initials undefined → the unassigned state (dashed ring + tiny muted user glyph).
export const Avatar: React.FC<{ initials?: string; size?: number }> = ({
  initials,
  size = 18,
}) => {
  if (!initials) {
    return (
      <span
        style={{
          width: size,
          height: size,
          flex: `none`,
          display: `inline-flex`,
          alignItems: `center`,
          justifyContent: `center`,
          borderRadius: 999,
          border: `1px dashed rgba(255,255,255,0.25)`,
          color: `rgba(250,250,250,0.35)`,
        }}
      >
        <UserIcon size={Math.round(size * 0.55)} />
      </span>
    )
  }
  const accent = avatarAccent(initials)
  return (
    <span
      style={{
        width: size,
        height: size,
        flex: `none`,
        display: `inline-flex`,
        alignItems: `center`,
        justifyContent: `center`,
        borderRadius: 999,
        backgroundColor: accent.bg,
        color: accent.fg,
        fontFamily: UI_FONT,
        fontSize: Math.round(size * 0.42),
        fontWeight: 600,
        lineHeight: 1,
        letterSpacing: 0.2,
      }}
    >
      {initials}
    </span>
  )
}

export const LabelChip: React.FC<{ name: string; dot: string }> = ({
  name,
  dot,
}) => (
  <span
    style={{
      display: `inline-flex`,
      alignItems: `center`,
      gap: 5,
      flex: `none`,
      fontFamily: UI_FONT,
      fontSize: 12,
      color: C.muted,
      whiteSpace: `nowrap`,
      maxWidth: 92,
      overflow: `hidden`,
    }}
  >
    <span
      style={{
        width: 6,
        height: 6,
        flex: `none`,
        borderRadius: 999,
        backgroundColor: dot,
      }}
    />
    <span style={{ overflow: `hidden`, textOverflow: `ellipsis` }}>
      {name.charAt(0).toUpperCase() + name.slice(1)}
    </span>
  </span>
)
