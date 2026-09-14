// surfaces/session.tsx — SessionScreen: the RUN face of a top tab, as the
// desktop shows it since EXP-746/773/791/877. What streams in it is the
// agent's own narration and its tool calls over ACP — never a PTY.
//
// Anatomy (steer_viewer.rs feed + render_composer / render_composer_footer):
//   · the fixed WORK HEADER is the caller's (detail.tsx WorkHeader): one
//     header across the Issue and Run faces, its toggle carrying the run's
//     `+N -M` diff item — the bottom "Latest changes" band is GONE (EXP-877)
//   · the transcript in the work column (896px, ×1.09 here) — a sparkle glyph before
//     prose, a wrench before a tool row, your own steer as a right-aligned
//     message
//   · ONE composer, one row: the field and the round Send/Stop button (no
//     tools in the card), over a FOOTER that carries the attach `+` on the
//     left and the model pin + the context RING on the right; a pane-wide
//     hairline sits above it
//
// Window-local px, like the other ships surfaces: the 1568-wide window is
// 1.09× the captured 1440 desktop, so the app's rems carry that step
// (text_sm 12.25 → 13.5, text_xs 10.5 → 11.5).

import React from "react"
import { interpolate } from "remotion"
import { C, EASE, MONO_FONT, R, UI_FONT } from "../theme"
import type { SessionEvent } from "../fixtures"

const CLAMP = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const
const CLAMP_EASE = { ...CLAMP, easing: EASE } as const

const COMPOSER_H = 46
const FOOTER_H = 30
const MEASURE = 976 // work_header WORK_COLUMN_W 896 at the 1.09 step
const GUTTER = 16
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

// coding-assistant = sparkles, coding-tool = wrench, ui-stop = circle-stop,
// ui-add = plus (packages/icons/icons.json).
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

// usage_sheet::context_ring — a 16px progress circle, the glass foreground
// at 30% while the window is comfortable (usage_bar severity_color).
const ContextRing: React.FC<{ percent: number }> = ({ percent }) => {
  const r = 6.5
  const c = 2 * Math.PI * r
  return (
    <svg width={16} height={16} viewBox="0 0 16 16" style={{ display: "block" }}>
      <circle cx="8" cy="8" r={r} fill="none" stroke="rgba(255,255,255,0.10)" strokeWidth={2} />
      <circle
        cx="8"
        cy="8"
        r={r}
        fill="none"
        stroke="rgba(250,250,250,0.30)"
        strokeWidth={2}
        strokeDasharray={`${(c * percent) / 100} ${c}`}
        transform="rotate(-90 8 8)"
        strokeLinecap="round"
      />
    </svg>
  )
}

export type SessionScreenProps = {
  frame: number
  width: number
  height: number
  events: SessionEvent[]
  /** Global frame each event lands on (same length as `events`). */
  schedule: readonly number[]
  /** Global frame the composer pulses on — a steer landing from elsewhere. */
  inputGlow?: number
  composerPlaceholder?: string
  /** The footer's model pin (`config_state.options[model]`). */
  model?: string
  /** The context window's fill, 0–100. */
  contextPercent?: number
}

export const SessionScreen: React.FC<SessionScreenProps> = ({
  frame,
  width,
  height,
  events,
  schedule,
  inputGlow,
  composerPlaceholder = "Type / for commands",
  model = "Opus",
  contextPercent = 34,
}) => {
  const shown = events
    .map((event, i) => ({ event, at: schedule[i] ?? 0 }))
    .filter((row) => frame >= row.at)
  const glow =
    inputGlow === undefined
      ? 0
      : interpolate(frame, [inputGlow, inputGlow + 6, inputGlow + 22], [0, 1, 0], CLAMP)

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
      {/* the transcript in the work column */}
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
            maxWidth: MEASURE - 2 * GUTTER,
            display: "flex",
            flexDirection: "column",
          }}
        >
          {shown.map((row, i) => (
            <FeedRow key={i} event={row.event} frame={frame} at={row.at} />
          ))}
        </div>
      </div>

      {/* the composer band: a pane-wide hairline, the card + its footer in
          the work column */}
      <div
        style={{
          flex: "none",
          borderTop: `1px solid ${C.strokeRow}`,
          padding: `10px ${GUTTER}px 8px`,
          display: "flex",
          justifyContent: "center",
        }}
      >
        <div style={{ width: "100%", maxWidth: MEASURE - 2 * GUTTER }}>
          <div
            style={{
              height: COMPOSER_H,
              boxSizing: "border-box",
              display: "flex",
              alignItems: "center",
              gap: 8,
              padding: "0 7px 0 14px",
              borderRadius: 16,
              border: `1px solid ${glow > 0 ? C.strokeActive : C.strokeCard}`,
              backgroundColor: glow > 0 ? C.fillActive : C.fillCard,
              boxShadow: glow > 0 ? `0 0 0 ${2 * glow}px rgba(255,255,255,0.08)` : "none",
            }}
          >
            <span style={{ flex: 1, minWidth: 0, fontSize: TEXT_BODY, color: C.muted }}>
              {composerPlaceholder}
            </span>
            {/* the ONE round button — Stop while the agent works */}
            <span
              style={{
                width: 32,
                height: 32,
                flex: "none",
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                color: C.text,
              }}
            >
              <CircleStopIcon size={20} />
            </span>
          </div>
          {/* render_composer_footer: attach · · · model · context ring */}
          <div
            style={{
              height: FOOTER_H,
              display: "flex",
              alignItems: "center",
              gap: 6,
              padding: "4px 4px 0",
              color: C.muted,
              fontSize: 12,
            }}
          >
            <span style={{ width: 24, height: 24, display: "flex", alignItems: "center", justifyContent: "center" }}>
              <PlusIcon size={14} />
            </span>
            <span style={{ flex: 1 }} />
            <span style={{ padding: "0 6px" }}>{model}</span>
            <span style={{ width: 24, height: 24, display: "flex", alignItems: "center", justifyContent: "center" }}>
              <ContextRing percent={contextPercent} />
            </span>
          </div>
        </div>
      </div>
    </div>
  )
}
