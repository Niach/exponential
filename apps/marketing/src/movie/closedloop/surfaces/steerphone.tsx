// closedloop/surfaces/steerphone.tsx — the live-steer phone (EXP-337): an
// iPhone chassis (extracted from the retired platforms.tsx PhoneMock) whose
// screen shows the mobile steer activity view — the session feed mirrored
// onto the phone, a steer message being typed into the composer and sent.
// All frame props are COMPOSITION-LOCAL to the segment that renders it.

import React from "react"
import { interpolate, spring } from "remotion"
import { C, MONO_FONT, POP, UI_FONT } from "../../ships/theme"
import type { SteerItem } from "../../ships/fixtures"
import { typed, useBlink } from "../../ships/rig"
import { CL, CL_PHONE_FEED, CL_STEER_MSG } from "../fixtures"

const CLAMP = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const

export const PHONE = { w: 330, screenW: 312, screenH: 660 } as const
export const PHONE_TOTAL_H = PHONE.screenH + 18 // screen + bezel padding

const Glyph: React.FC<{
  size: number
  sw?: number
  children: React.ReactNode
}> = ({ size, sw = 1.9, children }) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth={sw}
    strokeLinecap="round"
    strokeLinejoin="round"
    style={{ display: "block", flexShrink: 0 }}
  >
    {children}
  </svg>
)

// ── The bare chassis: bezel, screen, status row, dynamic island ──────────────
export const PhoneChassis: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => (
  <div
    style={{
      width: PHONE.w,
      padding: 9,
      boxSizing: "border-box",
      borderRadius: 44,
      backgroundColor: "#050505",
      border: "1px solid rgba(255,255,255,0.14)",
      boxShadow: "0 30px 70px rgba(0,0,0,0.55), 0 4px 18px rgba(0,0,0,0.4)",
      fontFamily: UI_FONT,
    }}
  >
    <div
      style={{
        position: "relative",
        width: PHONE.screenW,
        height: PHONE.screenH,
        borderRadius: 36,
        overflow: "hidden",
        backgroundColor: C.bg,
      }}
    >
      {/* status row */}
      <div
        style={{
          position: "absolute",
          top: 12,
          left: 26,
          right: 26,
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          zIndex: 5,
        }}
      >
        <span
          style={{
            fontSize: 14,
            fontWeight: 600,
            color: C.text,
            letterSpacing: 0.2,
          }}
        >
          9:41
        </span>
        <span
          style={{
            display: "flex",
            alignItems: "center",
            gap: 5,
            color: C.text,
          }}
        >
          <Glyph size={14}>
            <path d="M3 9.5C8.2 4.6 15.8 4.6 21 9.5" />
            <path d="M6.2 13.2c3.4-3.1 8.2-3.1 11.6 0" />
            <circle cx={12} cy={18} r={1.3} fill="currentColor" stroke="none" />
          </Glyph>
          <span
            style={{
              width: 23,
              height: 12,
              borderRadius: 3.5,
              border: "1px solid rgba(255,255,255,0.5)",
              padding: 1.5,
              boxSizing: "border-box",
            }}
          >
            <span
              style={{
                display: "block",
                width: "72%",
                height: "100%",
                borderRadius: 1.5,
                backgroundColor: C.text,
              }}
            />
          </span>
        </span>
      </div>
      {/* dynamic island */}
      <div
        style={{
          position: "absolute",
          top: 9,
          left: (PHONE.screenW - 88) / 2,
          width: 88,
          height: 25,
          borderRadius: 999,
          backgroundColor: "#000000",
          border: "1px solid rgba(255,255,255,0.05)",
          zIndex: 5,
        }}
      />
      {children}
    </div>
  </div>
)

// ── Steer feed rows (iOS activity-view grammar) ──────────────────────────────
const reveal = (frame: number, at: number): React.CSSProperties => ({
  opacity: interpolate(frame, [at, at + 5], [0, 1], CLAMP),
  translate: `0px ${interpolate(frame, [at, at + 5], [8, 0], CLAMP)}px`,
})

const ToolRow: React.FC<{
  frame: number
  at: number
  name: string
  summary?: string
}> = ({ frame, at, name, summary }) => {
  if (frame < at) return null
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 8,
        padding: "5px 0",
        ...reveal(frame, at),
      }}
    >
      <span style={{ color: C.muted, display: "flex" }}>
        <Glyph size={13} sw={2}>
          <path d="m16 18 6-6-6-6" />
          <path d="m8 6-6 6 6 6" />
        </Glyph>
      </span>
      <span
        style={{
          fontSize: 12.5,
          fontWeight: 600,
          color: C.text,
          flexShrink: 0,
        }}
      >
        {name}
      </span>
      {summary ? (
        <span
          style={{
            fontFamily: MONO_FONT,
            fontSize: 10.5,
            color: C.dim,
            whiteSpace: "nowrap",
            overflow: "hidden",
            textOverflow: "ellipsis",
          }}
        >
          {summary}
        </span>
      ) : null}
    </div>
  )
}

const NarrationRow: React.FC<{ frame: number; at: number; text: string }> = ({
  frame,
  at,
  text,
}) => {
  if (frame < at) return null
  return (
    <div
      style={{
        display: "flex",
        gap: 8,
        padding: "5px 0",
        ...reveal(frame, at),
      }}
    >
      <span style={{ color: C.indigoGlow, display: "flex", marginTop: 2 }}>
        <Glyph size={12} sw={1.8}>
          <path d="M12 3v3M12 18v3M3 12h3M18 12h3M5.6 5.6l2.1 2.1M16.3 16.3l2.1 2.1M18.4 5.6l-2.1 2.1M7.7 16.3l-2.1 2.1" />
        </Glyph>
      </span>
      <span
        style={{
          flex: 1,
          fontSize: 12,
          lineHeight: 1.5,
          color: C.muted,
          backgroundColor: C.panel,
          border: `1px solid ${C.borderSoft}`,
          borderRadius: 12,
          borderTopLeftRadius: 4,
          padding: "7px 10px",
        }}
      >
        {text}
      </span>
    </div>
  )
}

// ── The steer screen ─────────────────────────────────────────────────────────
export const SteerPhone: React.FC<{
  frame: number
  feedSchedule: readonly number[] // per CL_PHONE_FEED item
  typeAt: number // steer message typing starts (cpf 2)
  sendAt: number // send tap: bubble lands in the feed, composer clears
  items?: readonly SteerItem[]
  message?: string
}> = ({
  frame,
  feedSchedule,
  typeAt,
  sendAt,
  items = CL_PHONE_FEED,
  message = CL_STEER_MSG,
}) => {
  const blinkOn = useBlink(frame)
  const typedMsg = frame >= sendAt ? "" : typed(message, frame, typeAt, 2)
  const typingDone = frame >= typeAt + Math.ceil(message.length / 2)
  const sent = frame >= sendAt
  const sendPop = sent
    ? spring({ frame: frame - sendAt, fps: 30, config: POP })
    : 0
  return (
    <PhoneChassis>
      {/* header: session title + live badge */}
      <div
        style={{
          position: "absolute",
          top: 44,
          left: 16,
          right: 16,
          zIndex: 2,
        }}
      >
        <div
          style={{
            fontSize: 14.5,
            fontWeight: 600,
            color: C.text,
            whiteSpace: "nowrap",
            overflow: "hidden",
            textOverflow: "ellipsis",
          }}
        >
          {CL.sessionTab}
        </div>
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 5,
            marginTop: 3,
          }}
        >
          <span
            style={{
              width: 6,
              height: 6,
              borderRadius: 999,
              backgroundColor: C.green,
            }}
          />
          <span
            style={{ fontSize: 11, color: C.muted }}
          >{`Live · Rileys-MacBook-Pro.local`}</span>
        </div>
      </div>

      {/* the activity feed */}
      <div
        style={{
          position: "absolute",
          top: 96,
          left: 16,
          right: 16,
          bottom: 96,
          display: "flex",
          flexDirection: "column",
          justifyContent: "flex-end",
          overflow: "hidden",
        }}
      >
        {items.map((item, i) => {
          const at = feedSchedule[i] ?? 0
          return item.kind === "tool" ? (
            <ToolRow
              key={i}
              frame={frame}
              at={at}
              name={item.name}
              summary={item.summary}
            />
          ) : (
            <NarrationRow key={i} frame={frame} at={at} text={item.text} />
          )
        })}
        {/* the sent steer message — right-aligned user bubble */}
        {sent ? (
          <div
            style={{
              display: "flex",
              justifyContent: "flex-end",
              padding: "6px 0",
              scale: String(0.85 + 0.15 * sendPop),
              opacity: Math.min(1, sendPop * 2),
              transformOrigin: "bottom right",
            }}
          >
            <span
              style={{
                maxWidth: "88%",
                fontSize: 12,
                lineHeight: 1.5,
                color: "#ffffff",
                backgroundColor: C.indigo,
                borderRadius: 12,
                borderBottomRightRadius: 4,
                padding: "7px 10px",
              }}
            >
              {message}
            </span>
          </div>
        ) : null}
      </div>

      {/* composer */}
      <div
        style={{
          position: "absolute",
          left: 12,
          right: 12,
          bottom: 16,
          display: "flex",
          alignItems: "flex-end",
          gap: 8,
        }}
      >
        <div
          style={{
            flex: 1,
            minHeight: 40,
            boxSizing: "border-box",
            padding: "9px 12px",
            borderRadius: 20,
            backgroundColor: C.panel,
            border: `1px solid ${frame >= typeAt && !sent ? "rgba(99,102,241,0.55)" : C.border}`,
            fontSize: 12,
            lineHeight: 1.5,
            color: typedMsg ? C.text : C.dim,
          }}
        >
          {typedMsg || (sent || frame < typeAt ? "Steer the agent…" : "")}
          {!sent && frame >= typeAt ? (
            <span
              style={{
                display: "inline-block",
                width: 6,
                height: 12,
                marginLeft: 1,
                verticalAlign: "-2px",
                backgroundColor: C.text,
                opacity: !typingDone || blinkOn ? 1 : 0,
              }}
            />
          ) : null}
        </div>
        <span
          style={{
            width: 40,
            height: 40,
            flexShrink: 0,
            borderRadius: 999,
            backgroundColor: typedMsg || sent ? C.indigo : C.accentBg,
            color: typedMsg || sent ? "#ffffff" : C.dim,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            scale: sent
              ? String(1 + 0.12 * Math.max(0, 1 - (frame - sendAt) / 8))
              : "1",
          }}
        >
          <Glyph size={16} sw={2.2}>
            <path d="m5 12 7-7 7 7" />
            <path d="M12 19V5" />
          </Glyph>
        </span>
      </div>
    </PhoneChassis>
  )
}
