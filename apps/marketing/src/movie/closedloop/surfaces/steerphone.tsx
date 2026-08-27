// closedloop/surfaces/steerphone.tsx — the live-steer phone: an iPhone
// chassis whose screen shows the REAL mobile session screen (EXP-388,
// AgentSessionView twin): nav bar with a status dot + "Live · MacBook Pro"
// (no issue title up there), a bottom-anchored activity feed of wrench tool
// rows and bubble-less sparkles narration, a trailing white user bubble, and
// the "Message the agent…" composer with the small glass send capsule.
// All frame props are COMPOSITION-LOCAL to the segment that renders it.

import React from "react"
import { interpolate, spring } from "remotion"
import { C, GLASS, MONO_FONT, POP, UI_FONT } from "../../ships/theme"
import type { SteerItem } from "../../ships/fixtures"
import { typed, useBlink, wallpaperBackground } from "../../ships/rig"
import {
  CL_FILE_STATS,
  CL_PHONE_FEED,
  CL_STEER_MSG,
  PHONE_START,
} from "../fixtures"

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
// `glass` = the chassis' comp position: the screen then renders the EXP-359
// glass recipe (wallpaper replica + page gradient at the mobile alpha) instead
// of the solid gradient, matching the IDE window. The replica offset ignores
// any phone scale transform — on the soft wallpaper gradient the slight zoom
// is invisible, and a real backdrop-filter is off-limits (see rig.tsx).
export const PhoneChassis: React.FC<{
  children: React.ReactNode
  glass?: { x: number; y: number }
  hideStatus?: boolean // lock-screen shots carry their own big clock
}> = ({ children, glass, hideStatus }) => (
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
        backgroundColor: glass ? C.canvas : undefined,
        background: glass
          ? undefined
          : `linear-gradient(to bottom, #09090b, #18181b)`,
      }}
    >
      {glass ? (
        <>
          <div
            style={{
              position: "absolute",
              inset: 0,
              background: wallpaperBackground(-(glass.x + 9), -(glass.y + 9)),
            }}
          />
          <div
            style={{
              position: "absolute",
              inset: 0,
              background: `linear-gradient(to bottom, rgba(18,18,21,${GLASS.phoneAlpha}), rgba(24,24,27,${GLASS.phoneAlpha}))`,
            }}
          />
        </>
      ) : null}
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
            opacity: hideStatus ? 0 : 1,
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

// ── Steer feed rows (real AgentSessionView grammar) ──────────────────────────
const reveal = (frame: number, at: number): React.CSSProperties => ({
  opacity: interpolate(frame, [at, at + 5], [0, 1], CLAMP),
  translate: `0px ${interpolate(frame, [at, at + 5], [8, 0], CLAMP)}px`,
})

// Tool row: wrench glyph · tool name · middle-truncated mono detail.
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
      <span style={{ color: C.dim, display: "flex" }}>
        <Glyph size={12} sw={2}>
          <path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z" />
        </Glyph>
      </span>
      <span
        style={{
          fontSize: 12,
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

// Narration: sparkles glyph + plain prose — NO bubble in the real app.
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
      <span style={{ color: C.dim, display: "flex", marginTop: 2 }}>
        <Glyph size={12} sw={1.8}>
          <path d="M9.9 2.9 11 6l3.1 1.1L11 8.2 9.9 11.3 8.8 8.2 5.7 7.1 8.8 6z" />
          <path d="M18 10l.9 2.1L21 13l-2.1.9L18 16l-.9-2.1L15 13l2.1-.9z" />
          <path d="M8 15l1 2.5L11.5 18.5 9 19.5 8 22l-1-2.5L4.5 18.5 7 17.5z" />
        </Glyph>
      </span>
      <span
        style={{
          flex: 1,
          fontSize: 12,
          lineHeight: 1.5,
          color: C.muted,
        }}
      >
        {text}
      </span>
    </div>
  )
}

// ── The session screen ───────────────────────────────────────────────────────
export const SteerPhone: React.FC<{
  frame: number
  feedSchedule: readonly number[] // per CL_PHONE_FEED item
  typeAt: number // steer message typing starts (cpf 2)
  sendAt: number // send tap: bubble lands in the feed, composer clears
  items?: readonly SteerItem[]
  message?: string
  glass?: { x: number; y: number } // chassis comp position (see PhoneChassis)
}> = ({
  frame,
  feedSchedule,
  typeAt,
  sendAt,
  items = CL_PHONE_FEED,
  message = CL_STEER_MSG,
  glass,
}) => {
  const blinkOn = useBlink(frame)
  const typedMsg = frame >= sendAt ? "" : typed(message, frame, typeAt, 2)
  const typingDone = frame >= typeAt + Math.ceil(message.length / 2)
  const sent = frame >= sendAt
  const sendPop = sent
    ? spring({ frame: frame - sendAt, fps: 30, config: POP })
    : 0
  return (
    <PhoneChassis glass={glass}>
      {/* nav bar (shots/steering/ios): circled back · status dot + state ·
          host · the circled red kill-session button */}
      <div
        style={{
          position: "absolute",
          top: 40,
          left: 12,
          right: 12,
          height: 34,
          display: "flex",
          alignItems: "center",
          color: C.muted,
          zIndex: 2,
        }}
      >
        <span
          style={{
            width: 34,
            height: 34,
            flexShrink: 0,
            borderRadius: 999,
            backgroundColor: C.fillCard,
            border: `1px solid rgba(255,255,255,0.07)`,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            color: C.text,
          }}
        >
          <Glyph size={16} sw={2.2}>
            <path d="m15 18-6-6 6-6" />
          </Glyph>
        </span>
        <span
          style={{
            position: "absolute",
            left: 42,
            right: 42,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            gap: 7,
          }}
        >
          <span
            style={{
              width: 8,
              height: 8,
              flexShrink: 0,
              borderRadius: 999,
              backgroundColor: C.green,
            }}
          />
          <span
            style={{
              fontSize: 12,
              fontWeight: 500,
              color: C.muted,
              whiteSpace: "nowrap",
            }}
          >{`Live · ${PHONE_START.device}`}</span>
        </span>
        {/* kill-session button (owner-only in the app) */}
        <span
          style={{
            marginLeft: "auto",
            width: 34,
            height: 34,
            flexShrink: 0,
            borderRadius: 999,
            backgroundColor: C.fillCard,
            border: `1px solid rgba(255,255,255,0.07)`,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            color: C.destructive,
          }}
        >
          <Glyph size={16} sw={2}>
            <circle cx="12" cy="12" r="9" />
            <path d="m15 9-6 6" />
            <path d="m9 9 6 6" />
          </Glyph>
        </span>
      </div>

      {/* the activity feed */}
      <div
        style={{
          position: "absolute",
          top: 82,
          left: 14,
          right: 14,
          bottom: 132,
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
        {/* the sent steer message — trailing user bubble (white 10%) */}
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
                color: C.text,
                backgroundColor: "rgba(255,255,255,0.10)",
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

      {/* the pinned "Latest changes" diffstat strip + the composer: ONE tall
          rounded field with the plus affordance bottom-left and send
          bottom-right (shots/steering/ios) */}
      <div
        style={{
          position: "absolute",
          left: 12,
          right: 12,
          bottom: 16,
        }}
      >
        <div
          style={{
            height: 32,
            marginBottom: 8,
            boxSizing: "border-box",
            display: "flex",
            alignItems: "center",
            gap: 8,
            padding: "0 12px",
            borderRadius: 12,
            backgroundColor: "rgba(255,255,255,0.05)",
            border: `1px solid rgba(255,255,255,0.07)`,
            color: C.muted,
          }}
        >
          <Glyph size={13} sw={1.8}>
            <rect x="4" y="3" width="16" height="18" rx="2" />
            <path d="M12 8v6" />
            <path d="M9 11h6" />
          </Glyph>
          <span style={{ flex: 1, fontSize: 12, color: C.text }}>
            Latest changes
          </span>
          <span style={{ fontSize: 11.5, color: C.diffAdd }}>
            {`+${CL_FILE_STATS.add}`}
          </span>
          <span style={{ fontSize: 11.5, color: C.destructive }}>
            {`\u2212${CL_FILE_STATS.del}`}
          </span>
          <Glyph size={12} sw={2}>
            <path d="m18 15-6-6-6 6" />
          </Glyph>
        </div>
        <div
          style={{
            boxSizing: "border-box",
            padding: "10px 12px 8px",
            borderRadius: 16,
            backgroundColor: "rgba(255,255,255,0.06)",
            border: `1px solid ${frame >= typeAt && !sent ? "rgba(255,255,255,0.45)" : "rgba(255,255,255,0.10)"}`,
          }}
        >
          <div
            style={{
              minHeight: 17,
              fontSize: 12,
              lineHeight: 1.45,
              color: typedMsg ? C.text : C.dim,
            }}
          >
            {typedMsg || (sent || frame < typeAt ? "Message the agent…" : "")}
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
          <div
            style={{
              marginTop: 8,
              height: 22,
              display: "flex",
              alignItems: "center",
              color: C.muted,
            }}
          >
            <Glyph size={16} sw={2}>
              <path d="M5 12h14" />
              <path d="M12 5v14" />
            </Glyph>
            <span style={{ flex: 1 }} />
            <span
              style={{
                display: "flex",
                color: typedMsg || sent ? C.text : C.dim,
                opacity: typedMsg || sent ? 1 : 0.6,
                scale: sent
                  ? String(1 + 0.12 * Math.max(0, 1 - (frame - sendAt) / 8))
                  : "1",
              }}
            >
              <Glyph size={17} sw={1.9}>
                <path d="M14.536 21.686a.5.5 0 0 0 .937-.024l6.5-19a.496.496 0 0 0-.635-.635l-19 6.5a.5.5 0 0 0-.024.937l7.93 3.18a2 2 0 0 1 1.112 1.11z" />
                <path d="m21.854 2.147-10.94 10.939" />
              </Glyph>
            </span>
          </div>
        </div>
      </div>
    </PhoneChassis>
  )
}
