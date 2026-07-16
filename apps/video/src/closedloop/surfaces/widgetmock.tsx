// closedloop/surfaces/widgetmock.tsx — the embeddable feedback widget panel,
// modeled on packages/widget/src/ui (+ widget.css: 380px card, dark zinc theme,
// screenshot thumb with Annotate/Retake chips, Title/Details fields, full-width
// send button, powered-by footer, success check view). The panel floats above
// the FAB bottom-right of the light site — its dark card is the first taste of
// the product palette. All frame props are COMPOSITION-GLOBAL.

import React from "react"
import { interpolate, spring } from "remotion"
import { EASE, MONO_FONT, SETTLE, UI_FONT, WIN } from "../../ships/theme"
import { typed, useBlink } from "../../ships/rig"
import { REPORT } from "../fixtures"
import { CheckoutPage, PAGE_H, SITE_ANCHORS } from "./sitemock"

const CLAMP = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const
const EASED = { ...CLAMP, easing: EASE } as const

// Widget palette (packages/widget/src/theme.ts).
const W = {
  card: "#171717",
  bg: "#0a0a0a",
  fg: "#fafafa",
  muted: "#a3a3a3",
  border: "rgba(255,255,255,0.1)",
  input: "rgba(255,255,255,0.15)",
  success: "#22c55e",
  accent: "#e5e5e5",
  accentFg: "#171717",
  annotate: "#ef4444",
} as const

// ── Geometry (window-local panel rect; children in panel-local coords) ────────
export const PANEL_RECT = { x: 1168, y: 422, w: 380, h: 482 } as const
const PAD = 16
const INNER_W = PANEL_RECT.w - 2 * PAD // 348
const HEADER_H = 44
const SHOT = { x: PAD, y: 60, w: INNER_W, h: 170 }
const TITLE_INPUT = { x: PAD, y: 260, w: INNER_W, h: 36 }
const DETAILS_INPUT = { x: PAD, y: 326, w: INNER_W, h: 64 }
const SEND = { x: PAD, y: 404, w: INNER_W, h: 38 }

export const WIDGET_ANCHORS = {
  close: { x: PANEL_RECT.x + PANEL_RECT.w - 24, y: PANEL_RECT.y + 22 },
  titleInput: { x: PANEL_RECT.x + TITLE_INPUT.x + TITLE_INPUT.w / 2, y: PANEL_RECT.y + TITLE_INPUT.y + TITLE_INPUT.h / 2 },
  detailsInput: { x: PANEL_RECT.x + DETAILS_INPUT.x + DETAILS_INPUT.w / 2, y: PANEL_RECT.y + DETAILS_INPUT.y + DETAILS_INPUT.h / 2 },
  send: { x: PANEL_RECT.x + SEND.x + SEND.w / 2, y: PANEL_RECT.y + SEND.y + SEND.h / 2 },
} as const

// Screenshot thumbnail transform: the full 1568-wide page scaled to the thumb
// width, nudged up so the pay-button region sits comfortably in the crop.
const MINI_S = SHOT.w / WIN.w // ≈ 0.222
const MINI_DY = -18

// ── Tiny glyphs ───────────────────────────────────────────────────────────────
const XIcon: React.FC<{ size?: number }> = ({ size = 13 }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round">
    <path d="M18 6 6 18M6 6l12 12" />
  </svg>
)

const CheckDraw: React.FC<{ size: number; stroke: string; drawT: number; sw?: number }> = ({ size, stroke, drawT, sw = 2.6 }) => {
  const LEN = 24
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke={stroke} strokeWidth={sw} strokeLinecap="round" strokeLinejoin="round">
      <path d="M4.5 12.5 10 18 19.5 6.5" strokeDasharray={LEN} strokeDashoffset={LEN * (1 - Math.min(1, Math.max(0, drawT)))} />
    </svg>
  )
}

const Spinner: React.FC<{ frame: number; start: number; size?: number }> = ({ frame, start, size = 13 }) => (
  <svg width={size} height={size} viewBox="0 0 16 16" style={{ rotate: `${(Math.max(0, frame - start) * 30) % 360}deg` }}>
    <circle cx={8} cy={8} r={6} fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeDasharray="26 12" />
  </svg>
)

// ── Field primitives (panel-local) ────────────────────────────────────────────
const FieldLabel: React.FC<{ y: number; children: React.ReactNode }> = ({ y, children }) => (
  <div style={{ position: "absolute", left: PAD, top: y, fontSize: 12, fontWeight: 500, color: W.muted }}>{children}</div>
)

const FieldBox: React.FC<{
  rect: { x: number; y: number; w: number; h: number }
  focused: boolean
  children: React.ReactNode
}> = ({ rect, focused, children }) => (
  <div
    style={{
      position: "absolute",
      left: rect.x,
      top: rect.y,
      width: rect.w,
      height: rect.h,
      boxSizing: "border-box",
      borderRadius: 8,
      border: `1px solid ${focused ? W.accent : W.input}`,
      backgroundColor: W.bg,
      display: "flex",
      alignItems: rect.h > 44 ? "flex-start" : "center",
      padding: rect.h > 44 ? "8px 10px" : "0 10px",
      fontSize: 13.5,
      color: W.fg,
      overflow: "hidden",
    }}
  >
    {children}
  </div>
)

const Caret: React.FC<{ on: boolean }> = ({ on }) => (
  <span style={{ display: "inline-block", width: 1.5, height: 16, marginLeft: 1, backgroundColor: W.fg, opacity: on ? 1 : 0, flexShrink: 0 }} />
)

// ── The panel ─────────────────────────────────────────────────────────────────
export type WidgetPanelProps = {
  frame: number
  appearAt: number // spring in (origin bottom-right, above the FAB)
  annotateAt?: number // red ellipse dash-draws around the dead button in the thumb
  titleTypeAt?: number // Title text types (2 cpf)
  detailsTypeAt?: number // Details text types (2 cpf)
  sendHoverAt?: number
  sendingAt?: number // button → spinner + "Sending…"
  successAt?: number // form crossfades to the success check view
}

export const WidgetPanel: React.FC<WidgetPanelProps> = ({
  frame,
  appearAt,
  annotateAt,
  titleTypeAt,
  detailsTypeAt,
  sendHoverAt,
  sendingAt,
  successAt,
}) => {
  const blinkOn = useBlink(frame) // pure frame math, but keep it before the early return for the hooks rule
  if (frame < appearAt) return null
  const pop = spring({ frame: frame - appearAt, fps: 30, config: SETTLE })
  const o = interpolate(frame, [appearAt, appearAt + 6], [0, 1], CLAMP)

  const titleText = titleTypeAt === undefined ? "" : typed(REPORT.title, frame, titleTypeAt, 2)
  const detailsText = detailsTypeAt === undefined ? "" : typed(REPORT.details, frame, detailsTypeAt, 2)
  const titleFocused = titleTypeAt !== undefined && frame >= titleTypeAt - 6 && (detailsTypeAt === undefined || frame < detailsTypeAt - 6)
  const detailsFocused = detailsTypeAt !== undefined && frame >= detailsTypeAt - 6 && (sendingAt === undefined || frame < sendingAt)

  const successT = successAt === undefined ? 0 : interpolate(frame, [successAt, successAt + 8], [0, 1], EASED)
  const formO = 1 - successT

  const annotateT = annotateAt === undefined ? 0 : interpolate(frame, [annotateAt, annotateAt + 16], [0, 1], EASED)
  const sending = sendingAt !== undefined && frame >= sendingAt
  const hoverT = sendHoverAt === undefined ? 0 : interpolate(frame, [sendHoverAt, sendHoverAt + 4], [0, 1], CLAMP)

  const pay = SITE_ANCHORS.payRectPage
  const ell = {
    cx: (pay.x + pay.w / 2) * MINI_S,
    cy: (pay.y + pay.h / 2) * MINI_S + MINI_DY,
    rx: (pay.w / 2) * MINI_S + 14,
    ry: (pay.h / 2) * MINI_S + 8,
  }

  return (
    <div
      style={{
        position: "absolute",
        left: PANEL_RECT.x,
        top: PANEL_RECT.y,
        width: PANEL_RECT.w,
        height: PANEL_RECT.h,
        boxSizing: "border-box",
        borderRadius: 10,
        border: `1px solid ${W.border}`,
        backgroundColor: W.card,
        boxShadow: "0 12px 40px rgba(0,0,0,0.55)",
        overflow: "hidden",
        fontFamily: UI_FONT,
        opacity: o,
        scale: String(0.92 + 0.08 * pop),
        translate: `0px ${10 * (1 - pop)}px`,
        transformOrigin: "100% 100%",
        zIndex: 30,
      }}
    >
      {/* header */}
      <div
        style={{
          position: "absolute",
          top: 0,
          left: 0,
          right: 0,
          height: HEADER_H,
          boxSizing: "border-box",
          borderBottom: `1px solid ${W.border}`,
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          padding: "0 16px",
        }}
      >
        <span style={{ fontSize: 14, fontWeight: 600, color: W.fg }}>{REPORT.panelTitle}</span>
        <span style={{ color: W.muted, display: "flex" }}>
          <XIcon size={13} />
        </span>
      </div>

      {/* ── form (fades out on success) ── */}
      <div style={{ position: "absolute", inset: 0, opacity: formO }}>
        {/* screenshot thumb */}
        <div
          style={{
            position: "absolute",
            left: SHOT.x,
            top: SHOT.y,
            width: SHOT.w,
            height: SHOT.h,
            boxSizing: "border-box",
            borderRadius: 8,
            border: `1px solid ${W.border}`,
            backgroundColor: W.bg,
            overflow: "hidden",
          }}
        >
          <div
            style={{
              position: "absolute",
              left: 0,
              top: 0,
              width: WIN.w,
              height: PAGE_H,
              scale: String(MINI_S),
              translate: `0px ${MINI_DY}px`,
              transformOrigin: "0 0",
            }}
          >
            <CheckoutPage frame={0} />
          </div>
          {/* the red annotation ellipse (dash-draw) */}
          {annotateT > 0 ? (
            <svg width={SHOT.w} height={SHOT.h} viewBox={`0 0 ${SHOT.w} ${SHOT.h}`} style={{ position: "absolute", inset: 0 }}>
              <ellipse
                cx={ell.cx}
                cy={ell.cy}
                rx={ell.rx}
                ry={ell.ry}
                fill="none"
                stroke={W.annotate}
                strokeWidth={2.5}
                strokeLinecap="round"
                pathLength={100}
                strokeDasharray={100}
                strokeDashoffset={100 * (1 - annotateT)}
                transform={`rotate(-3 ${ell.cx} ${ell.cy})`}
              />
            </svg>
          ) : null}
          {/* Annotate / Retake chips */}
          <div style={{ position: "absolute", top: 6, right: 6, display: "flex", gap: 6 }}>
            {["Annotate", "Retake"].map((chip) => (
              <span
                key={chip}
                style={{
                  fontSize: 11.5,
                  fontWeight: 500,
                  padding: "4px 8px",
                  borderRadius: 6,
                  backgroundColor: "rgba(0,0,0,0.65)",
                  border: `1px solid ${W.border}`,
                  color: W.fg,
                }}
              >
                {chip}
              </span>
            ))}
          </div>
        </div>

        {/* Title */}
        <FieldLabel y={240}>{REPORT.titleLabel}</FieldLabel>
        <FieldBox rect={TITLE_INPUT} focused={titleFocused}>
          <span style={{ whiteSpace: "nowrap", overflow: "hidden" }}>{titleText}</span>
          {titleFocused ? <Caret on={blinkOn || titleText.length < REPORT.title.length} /> : null}
        </FieldBox>

        {/* Details */}
        <FieldLabel y={306}>{REPORT.detailsLabel}</FieldLabel>
        <FieldBox rect={DETAILS_INPUT} focused={detailsFocused}>
          <span style={{ lineHeight: 1.45 }}>
            {detailsText}
            {detailsFocused ? <Caret on={blinkOn || detailsText.length < REPORT.details.length} /> : null}
          </span>
        </FieldBox>

        {/* Send */}
        <div
          style={{
            position: "absolute",
            left: SEND.x,
            top: SEND.y,
            width: SEND.w,
            height: SEND.h,
            borderRadius: 8,
            backgroundColor: sending ? "rgba(229,229,229,0.85)" : hoverT > 0 ? `rgb(${Math.round(229 + 26 * hoverT)},${Math.round(229 + 26 * hoverT)},${Math.round(229 + 26 * hoverT)})` : W.accent,
            color: W.accentFg,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            gap: 7,
            fontSize: 13.5,
            fontWeight: 600,
          }}
        >
          {sending && sendingAt !== undefined ? (
            <>
              <Spinner frame={frame} start={sendingAt} />
              {REPORT.sending}
            </>
          ) : (
            REPORT.send
          )}
        </div>
      </div>

      {/* ── success view ── */}
      {successT > 0 && successAt !== undefined ? (
        <div
          style={{
            position: "absolute",
            left: 0,
            top: HEADER_H,
            right: 0,
            bottom: 40,
            display: "flex",
            flexDirection: "column",
            alignItems: "center",
            justifyContent: "center",
            gap: 10,
            opacity: successT,
          }}
        >
          <div
            style={{
              width: 44,
              height: 44,
              borderRadius: 999,
              border: `1.5px solid ${W.success}`,
              backgroundColor: "rgba(34,197,94,0.12)",
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              scale: String(0.8 + 0.2 * spring({ frame: frame - successAt, fps: 30, config: SETTLE })),
            }}
          >
            <CheckDraw size={20} stroke={W.success} drawT={interpolate(frame, [successAt + 2, successAt + 12], [0, 1], EASED)} />
          </div>
          <div style={{ fontSize: 14, fontWeight: 600, color: W.fg }}>{REPORT.successTitle}</div>
          <div style={{ fontSize: 12.5, color: W.muted, fontFamily: MONO_FONT }}>{REPORT.successSub}</div>
        </div>
      ) : null}

      {/* powered-by footer */}
      <div
        style={{
          position: "absolute",
          left: 0,
          right: 0,
          bottom: 10,
          textAlign: "center",
          fontSize: 11.5,
          color: W.muted,
        }}
      >
        Powered by <span style={{ fontWeight: 600, color: W.fg }}>Exponential</span>
      </div>
    </div>
  )
}
