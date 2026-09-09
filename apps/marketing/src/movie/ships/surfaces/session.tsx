// surfaces/session.tsx — SessionScreen: a coding run as the desktop shows it
// since EXP-746/773/791. What replaced the bottom terminal dock this file's
// predecessor drew: a run is a FULL-WIDTH screen (or, from an issue, the
// transcript sliding in over the issue's body), and what streams in it is the
// agent's own narration and its tool calls over ACP — never a PTY.
//
// Anatomy (session_screen.rs render_header / steer_viewer.rs feed /
// session_extras::changes_bar):
//   · header: status dot · mono identifier · subject · the phase caption
//     naming the machine · the Kill-session glyph
//   · the transcript in its reading column (design-tokens `transcript`:
//     736 measure, 48 gutters, prose 14/22, tool rows 12/18) — a sparkle
//     glyph before prose, a wrench before a tool row, your own steer as a
//     right-aligned message
//   · the collapsed "Latest changes" bar
//   · ONE composer: free text answers every card, and there is no mid-session
//     model/effort/plan control beside it (EXP-790)
//
// Window-local px, like the other ships surfaces: the 1568-wide window is
// 1.09× the captured 1440 desktop, so the app's rems carry that step
// (text_sm 12.25 → 13.5, text_xs 10.5 → 11.5, the 736 measure → 800).

import React from "react"
import { interpolate, spring } from "remotion"
import { C, EASE, MONO_FONT, POP, R, UI_FONT } from "../theme"
import type { SessionEvent } from "../fixtures"

const CLAMP = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const
const CLAMP_EASE = { ...CLAMP, easing: EASE } as const

const HEADER_H = 38
const CHANGES_H = 28
const COMPOSER_H = 44
const MEASURE = 800
const GUTTER = 52
const TEXT_BODY = 15
const LINE_BODY = 24
const TEXT_TOOL = 13
const LINE_TOOL = 20

type IconProps = { size?: number; sw?: number }
const Svg: React.FC<IconProps & { children: React.ReactNode }> = ({
  size = 13,
  sw = 1.7,
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
    style={{ display: "block" }}
  >
    {children}
  </svg>
)

// coding-assistant = sparkles, coding-tool = wrench, coding-stop = circle-x,
// ui-submit = circle-arrow-up, ui-file = file (packages/icons/icons.json).
const SparklesIcon: React.FC<IconProps> = (p) => (
  <Svg {...p}>
    <path d="M9.937 15.5A2 2 0 0 0 8.5 14.063l-6.135-1.582a.5.5 0 0 1 0-.962L8.5 9.936A2 2 0 0 0 9.937 8.5l1.582-6.135a.5.5 0 0 1 .963 0L14.063 8.5A2 2 0 0 0 15.5 9.937l6.135 1.581a.5.5 0 0 1 0 .964L15.5 14.063a2 2 0 0 0-1.437 1.437l-1.582 6.135a.5.5 0 0 1-.963 0z" />
  </Svg>
)
const WrenchIcon: React.FC<IconProps> = (p) => (
  <Svg {...p}>
    <path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z" />
  </Svg>
)
const CircleStopIcon: React.FC<IconProps> = (p) => (
  <Svg {...p}>
    <circle cx="12" cy="12" r="10" />
    <rect x="9" y="9" width="6" height="6" rx="1" />
  </Svg>
)
const CircleArrowUpIcon: React.FC<IconProps> = (p) => (
  <Svg {...p}>
    <circle cx="12" cy="12" r="10" />
    <path d="m16 12-4-4-4 4" />
    <path d="M12 16V8" />
  </Svg>
)
const FileIcon: React.FC<IconProps> = (p) => (
  <Svg {...p}>
    <path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" />
    <path d="M14 2v4a2 2 0 0 0 2 2h4" />
  </Svg>
)
const ChevronRightIcon: React.FC<IconProps> = (p) => (
  <Svg {...p}>
    <path d="m9 18 6-6-6-6" />
  </Svg>
)
const PlusIcon: React.FC<IconProps> = (p) => (
  <Svg {...p}>
    <path d="M12 5v14M5 12h14" />
  </Svg>
)

// ── One transcript row ───────────────────────────────────────────────────────
// The fixtures speak the session's own vocabulary (`SessionEvent`); the
// mapping to rows is the one every client makes: prose is narration, a tool
// call is "<Verb> <target> · <detail>", a spinner is the working row, a flash
// is the agent saying what it just landed, and a steer of yours is a message.
const FeedRow: React.FC<{
  event: SessionEvent
  frame: number
  at: number
}> = ({ event, frame, at }) => {
  const rise = interpolate(frame, [at, at + 8], [0, 1], CLAMP_EASE)
  const wrap = (children: React.ReactNode, gapTop: number): React.ReactElement => (
    <div
      style={{
        marginTop: gapTop,
        opacity: rise,
        translate: `0px ${(1 - rise) * 6}px`,
      }}
    >
      {children}
    </div>
  )
  const glyph = (Icon: React.FC<IconProps>, top: number) => (
    <span
      style={{
        flex: "none",
        marginTop: top,
        color: "rgba(161,161,161,0.6)",
        display: "flex",
      }}
    >
      <Icon size={12} />
    </span>
  )

  if (event.kind === "prose" || event.kind === "flash") {
    return wrap(
      <div style={{ display: "flex", gap: 8, alignItems: "flex-start" }}>
        {glyph(SparklesIcon, 4)}
        <div
          style={{
            minWidth: 0,
            fontSize: TEXT_BODY,
            lineHeight: `${LINE_BODY}px`,
            color: C.text,
          }}
        >
          {event.kind === "flash" ? event.text : event.text}
        </div>
      </div>,
      13
    )
  }
  if (event.kind === "spinner") {
    return wrap(
      <div
        style={{
          display: "flex",
          gap: 6,
          alignItems: "center",
          fontSize: TEXT_TOOL,
          lineHeight: `${LINE_TOOL}px`,
          color: C.muted,
        }}
      >
        <span
          style={{
            width: 10,
            height: 10,
            flex: "none",
            borderRadius: 999,
            border: "1.4px solid rgba(250,250,250,0.25)",
            borderTopColor: "rgba(250,250,250,0.75)",
            rotate: `${(frame * 12) % 360}deg`,
          }}
        />
        Working…
      </div>,
      9
    )
  }
  if (event.kind === "user") {
    // A message you sent — the transcript's own turn, right-aligned in the
    // reading column with the sent-turn gap on either side of it.
    return wrap(
      <div style={{ display: "flex", justifyContent: "flex-end" }}>
        <div
          style={{
            maxWidth: "82%",
            padding: "7px 11px",
            borderRadius: R.section,
            border: `1px solid ${C.strokeRow}`,
            backgroundColor: C.fillRow,
            fontSize: TEXT_BODY,
            lineHeight: `${LINE_BODY}px`,
            color: C.text,
          }}
        >
          {event.text}
        </div>
      </div>,
      17
    )
  }
  return wrap(
    <div
      style={{
        display: "flex",
        gap: 6,
        alignItems: "center",
        fontSize: TEXT_TOOL,
        lineHeight: `${LINE_TOOL}px`,
        color: C.text,
        minWidth: 0,
      }}
    >
      {glyph(WrenchIcon, 0)}
      <span style={{ flex: "none" }}>{event.tool}</span>
      <span
        style={{
          minWidth: 0,
          overflow: "hidden",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
          fontFamily: MONO_FONT,
          color: C.muted,
        }}
      >
        {event.args}
      </span>
      {event.result ? (
        <span style={{ flex: "none", color: C.muted }}>{`· ${event.result}`}</span>
      ) : null}
    </div>,
    9
  )
}

export type SessionScreenProps = {
  frame: number
  width: number
  height: number
  /** Mono identifier in the header (an issue run names its issue). */
  identifier?: string
  subject: string
  /** The phase caption — "Working · <machine>", "Needs your input · …". */
  caption: string
  /** The header dot's tone; amber while the run waits on you. */
  tone?: string
  events: SessionEvent[]
  /** Global frame each event lands on (same length as `events`). */
  schedule: readonly number[]
  /** Global frame the composer pulses on — a steer landing from elsewhere. */
  inputGlow?: number
  /** The changes bar's counts; omitted until the run has edited something. */
  changes?: { add: number; del: number; at: number }
  composerPlaceholder?: string
}

export const SessionScreen: React.FC<SessionScreenProps> = ({
  frame,
  width,
  height,
  identifier,
  subject,
  caption,
  tone = C.green,
  events,
  schedule,
  inputGlow,
  changes,
  composerPlaceholder = "Message the agent… (/ for commands)",
}) => {
  const shown = events
    .map((event, i) => ({ event, at: schedule[i] ?? 0 }))
    .filter((row) => frame >= row.at)
  const glow =
    inputGlow === undefined
      ? 0
      : interpolate(frame, [inputGlow, inputGlow + 6, inputGlow + 22], [0, 1, 0], CLAMP)
  const changesIn =
    changes === undefined
      ? 0
      : spring({ frame: frame - changes.at, fps: 30, config: POP })

  return (
    <div
      style={{
        position: "relative",
        width,
        height,
        overflow: "hidden",
        display: "flex",
        flexDirection: "column",
        fontFamily: UI_FONT,
        color: C.text,
      }}
    >
      {/* render_header */}
      <div
        style={{
          flex: "none",
          height: HEADER_H,
          boxSizing: "border-box",
          display: "flex",
          alignItems: "center",
          gap: 8,
          padding: "0 12px",
          borderBottom: `1px solid ${C.strokeRow}`,
        }}
      >
        <span
          style={{
            width: 6,
            height: 6,
            flex: "none",
            borderRadius: 999,
            backgroundColor: tone,
          }}
        />
        {identifier ? (
          <span
            style={{
              flex: "none",
              fontFamily: MONO_FONT,
              fontSize: 11.5,
              color: C.muted,
            }}
          >
            {identifier}
          </span>
        ) : null}
        <span
          style={{
            flex: "none",
            maxWidth: 380,
            fontSize: 13.5,
            whiteSpace: "nowrap",
            overflow: "hidden",
            textOverflow: "ellipsis",
          }}
        >
          {subject}
        </span>
        <span
          style={{
            flex: 1,
            minWidth: 0,
            fontSize: 11.5,
            color: C.muted,
            whiteSpace: "nowrap",
            overflow: "hidden",
            textOverflow: "ellipsis",
          }}
        >
          {caption}
        </span>
        <span style={{ flex: "none", color: C.muted, display: "flex" }}>
          <CircleStopIcon size={13} />
        </span>
      </div>

      {/* the transcript in its reading column */}
      <div
        style={{
          flex: 1,
          minHeight: 0,
          overflow: "hidden",
          padding: `14px ${GUTTER}px`,
          display: "flex",
          justifyContent: "center",
        }}
      >
        <div
          style={{
            width: "100%",
            maxWidth: MEASURE,
            display: "flex",
            flexDirection: "column",
          }}
        >
          {shown.map((row, i) => (
            <FeedRow key={i} event={row.event} frame={frame} at={row.at} />
          ))}
        </div>
      </div>

      {/* session_extras::changes_bar — collapsed, over the composer */}
      {changes && changesIn > 0 ? (
        <div
          style={{
            flex: "none",
            height: CHANGES_H,
            boxSizing: "border-box",
            display: "flex",
            alignItems: "center",
            gap: 6,
            padding: "0 12px",
            borderTop: `1px solid ${C.strokeRow}`,
            fontSize: TEXT_TOOL,
            opacity: changesIn,
          }}
        >
          <span style={{ color: C.dim, display: "flex", flex: "none" }}>
            <ChevronRightIcon size={12} />
          </span>
          <span style={{ color: "rgba(161,161,161,0.6)", display: "flex", flex: "none" }}>
            <FileIcon size={12} />
          </span>
          <span style={{ flex: 1, color: C.muted }}>Latest changes</span>
          <span style={{ fontFamily: MONO_FONT, color: C.diffAdd }}>
            {`+${changes.add}`}
          </span>
          <span style={{ fontFamily: MONO_FONT, color: C.diffDel }}>
            {`−${changes.del}`}
          </span>
        </div>
      ) : null}

      {/* the ONE composer */}
      <div
        style={{
          flex: "none",
          height: COMPOSER_H,
          boxSizing: "border-box",
          display: "flex",
          alignItems: "center",
          gap: 8,
          margin: "8px 12px 12px",
          padding: "0 12px",
          borderRadius: R.section,
          border: `1px solid ${glow > 0 ? C.strokeActive : C.strokeCard}`,
          backgroundColor: glow > 0 ? C.fillActive : C.fillRow,
          boxShadow: glow > 0 ? `0 0 0 ${2 * glow}px rgba(255,255,255,0.08)` : "none",
        }}
      >
        <span style={{ flex: 1, minWidth: 0, fontSize: TEXT_BODY, color: C.muted }}>
          {composerPlaceholder}
        </span>
        <span style={{ flex: "none", color: C.muted, display: "flex" }}>
          <PlusIcon size={13} />
        </span>
        <span style={{ flex: "none", color: C.primary, display: "flex" }}>
          <CircleArrowUpIcon size={17} />
        </span>
      </div>
    </div>
  )
}
