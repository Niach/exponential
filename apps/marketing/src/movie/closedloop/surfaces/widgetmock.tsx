// closedloop/surfaces/widgetmock.tsx — the embeddable feedback widget,
// COPIED from packages/widget/src/ui (Panel.tsx + Annotator.tsx + widget.css
// + theme.ts darkPalette). SLOP-4: ONE form — the panel opens PLAIN (the
// screenshot is on demand: "Attach a screenshot of this page." + the
// Take screenshot | ⏱ Off split chip), a capture lands in the full-screen
// annotator (rect/pen/arrow/crop · undo/clear/remove-crop · Cancel/Done),
// Done returns to the panel with the flattened thumbnail and its
// Annotate/Retake/Remove chips. The single "What happened?" message becomes
// the issue (title = its first line), "Email (optional)" sits under it, the
// "Send feedback" button is right-aligned in the footer and the
// "Powered by Exponential" strip closes the card. The success view has NO
// header. The panel is anchored like the real one: bottom 76px / right 20px
// of the page viewport, so it grows upward when the thumbnail appears.
// All frame props are COMPOSITION-GLOBAL (segment-local in practice).

import React from "react"
import { interpolate } from "remotion"
import { EASE, UI_FONT, WIN } from "../../ships/theme"
import { typed, useBlink } from "../../ships/rig"
import { WIDGET_COPY } from "../fixtures"
import { CHROME_H, CheckoutPage, PAGE_H, SITE_ANCHORS } from "./sitemock"

const CLAMP = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const
const EASED = { ...CLAMP, easing: EASE } as const

// Widget dark palette (packages/widget/src/theme.ts darkPalette).
const W = {
  card: "#171717",
  bg: "#0a0a0a",
  secondary: "#262626",
  fg: "#fafafa",
  muted: "#a3a3a3",
  border: "rgba(255,255,255,0.1)",
  input: "rgba(255,255,255,0.15)",
  destructive: "#ef4444",
  success: "#22c55e",
  accent: "#e5e5e5",
  accentFg: "#171717",
} as const

// ── Geometry (window-local; widget.css metrics) ───────────────────────────────
const PANEL_W = 380
const PANEL_RIGHT = 20 // .exp-panel.exp-right
const PANEL_BOTTOM = 76 // .exp-panel.exp-bottom (clears the corner FAB)
const PANEL_X = WIN.w - PANEL_RIGHT - PANEL_W // 1168
const PANEL_BOTTOM_Y = WIN.h - PANEL_BOTTOM // 904
const HEADER_H = 51 // padding 12 + 26px close + 12 + 1px rule
const BODY_PAD_Y = 14
const BODY_GAP = 12
const INNER_W = PANEL_W - 2 - 32 // 346
const SHOT_EMPTY_H = 96 // .exp-shot min-height
const SHOT_IMG_H = 180 // .exp-shot img max-height
const SHOT_FULL_H = SHOT_IMG_H + 2
const ADD_IMAGE_H = 28
const LABEL_H = 17.4 // 12px × 1.45
const TEXTAREA_H = 84
const INPUT_H = 34
const SUBMIT_H = 32
const POWERED_H = 31

const formHeight = (shotH: number) =>
  HEADER_H +
  BODY_PAD_Y +
  shotH +
  BODY_GAP +
  ADD_IMAGE_H +
  BODY_GAP +
  (LABEL_H + 4 + TEXTAREA_H) +
  BODY_GAP +
  (LABEL_H + 4 + INPUT_H) +
  BODY_GAP +
  SUBMIT_H +
  BODY_PAD_Y +
  POWERED_H
const TOP_EMPTY = PANEL_BOTTOM_Y - formHeight(SHOT_EMPTY_H)
const TOP_SHOT = PANEL_BOTTOM_Y - formHeight(SHOT_FULL_H)
const messageTop = (top: number, shotH: number) =>
  top + HEADER_H + BODY_PAD_Y + shotH + BODY_GAP + ADD_IMAGE_H + BODY_GAP + LABEL_H + 4

// The annotator (Annotator.tsx + widget.css .exp-annotator): a full-viewport
// overlay; toolbar 16px down, the stage fits the screenshot downscale-only
// into (stage − 16px). On the 1568×936 page: scale min(1552/1568, 858/936).
const ANN_TOOLBAR_TOP = 16
const ANN_TOOLBAR_H = 46
const ANN_STAGE_TOP = ANN_TOOLBAR_TOP + ANN_TOOLBAR_H // 62
const ANN_SCALE = Math.min((WIN.w - 16) / WIN.w, (PAGE_H - ANN_STAGE_TOP - 16) / PAGE_H)
const ANN_W = Math.round(WIN.w * ANN_SCALE)
const ANN_H = Math.round(PAGE_H * ANN_SCALE)
const ANN_LEFT = 16 + (WIN.w - 32 - (ANN_W + 2)) / 2
const ANN_TOP = ANN_STAGE_TOP + 16 + (PAGE_H - ANN_STAGE_TOP - 32 - (ANN_H + 2)) / 2
// The toolbar: 7 tools (32) + 2 separators (1 + 2×4 margin) + Cancel + Done,
// gap 4, padding 6, 1px border.
const ANN_CANCEL_W = 68
const ANN_DONE_W = 58
const ANN_TOOLBAR_W = 7 * 32 + 2 * 9 + ANN_CANCEL_W + ANN_DONE_W + 10 * 4 + 12 + 2
const ANN_TOOLBAR_X = (WIN.w - ANN_TOOLBAR_W) / 2

// The rect the reporter drags around the dead Pay button (page px).
const PAY = SITE_ANCHORS.payRectPage
const MARK = { x: PAY.x - 10, y: PAY.y - 10, w: PAY.w + 20, h: PAY.h + 20 }
// Annotation stroke: strokeWidthFor(1568, 936) = round(1568 × 0.0035) = 5 px.
const MARK_STROKE = Math.max(3, Math.round(WIN.w * 0.0035))
const annPoint = (x: number, y: number) => ({
  x: ANN_LEFT + 1 + x * ANN_SCALE,
  y: CHROME_H + ANN_TOP + 1 + y * ANN_SCALE,
})

// Take screenshot chip (empty state): the split chip group centered in the
// shot box (Take screenshot ≈104px + the ⏱ Off segment ≈45px).
const CHIP_GROUP_W = 149
const TAKE_W = 104
const SUBMIT_W = 124

export const WIDGET_ANCHORS = {
  takeScreenshot: {
    x: PANEL_X + PANEL_W / 2 - CHIP_GROUP_W / 2 + TAKE_W / 2,
    y: TOP_EMPTY + HEADER_H + BODY_PAD_Y + 61,
  },
  markFrom: annPoint(MARK.x, MARK.y),
  markTo: annPoint(MARK.x + MARK.w, MARK.y + MARK.h),
  annDone: {
    x: ANN_TOOLBAR_X + ANN_TOOLBAR_W - 1 - 6 - ANN_DONE_W / 2,
    y: CHROME_H + ANN_TOOLBAR_TOP + ANN_TOOLBAR_H / 2,
  },
  message: {
    x: PANEL_X + PANEL_W / 2,
    y: messageTop(TOP_SHOT, SHOT_FULL_H) + TEXTAREA_H / 2,
  },
  send: {
    x: PANEL_X + PANEL_W - 1 - 16 - SUBMIT_W / 2,
    y: PANEL_BOTTOM_Y - POWERED_H - BODY_PAD_Y - SUBMIT_H / 2,
  },
} as const

// ── Inline glyphs (the widget's own SVG strings) ──────────────────────────────
const Svg: React.FC<{ size: number; sw?: number; children: React.ReactNode }> = ({ size, sw = 2, children }) => (
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
const CloseIcon = () => (
  <Svg size={14}>
    <path d="M18 6 6 18M6 6l12 12" />
  </Svg>
)
const CheckIcon = () => (
  <Svg size={18} sw={2.5}>
    <path d="M20 6 9 17l-5-5" />
  </Svg>
)
const TimerIcon = () => (
  <Svg size={12}>
    <line x1="10" x2="14" y1="2" y2="2" />
    <line x1="12" x2="15" y1="14" y2="11" />
    <circle cx="12" cy="14" r="8" />
  </Svg>
)
const ANN_ICONS: React.ReactNode[] = [
  <rect key="r" x="4" y="6" width="16" height="12" rx="1" />,
  <path key="p" d="M3 19c3-6 5-9 7-9s2 7 4 7 4-8 7-12" />,
  <g key="a">
    <path d="M5 19 19 5" />
    <path d="M9 5h10v10" />
  </g>,
  <g key="c">
    <path d="M6 2v14a2 2 0 0 0 2 2h14" />
    <path d="M18 22V8a2 2 0 0 0-2-2H2" />
  </g>,
]
const ANN_ACTION_ICONS: React.ReactNode[] = [
  <g key="u">
    <path d="M3 7v6h6" />
    <path d="M3 13a9 9 0 1 0 3-7.7L3 7" />
  </g>,
  <g key="t">
    <path d="M3 6h18" />
    <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6" />
    <path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
  </g>,
  <g key="x">
    <path d="M6 2v14a2 2 0 0 0 2 2h14" />
    <path d="M18 22V8a2 2 0 0 0-2-2H2" />
    <path d="m2 2 20 20" />
  </g>,
]

// ── Primitives ────────────────────────────────────────────────────────────────
const chipStyle: React.CSSProperties = {
  fontSize: 11.5,
  fontWeight: 500,
  lineHeight: "normal",
  padding: "4px 8px",
  borderRadius: 6,
  backgroundColor: "rgba(0,0,0,0.65)",
  color: W.fg,
  border: `1px solid ${W.border}`,
  whiteSpace: "nowrap",
}

const Label: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <div style={{ fontSize: 12, fontWeight: 500, lineHeight: 1.45, color: W.muted }}>{children}</div>
)

const fieldStyle = (focused: boolean): React.CSSProperties => ({
  boxSizing: "border-box",
  width: "100%",
  fontSize: 13.5,
  lineHeight: "normal",
  color: W.fg,
  backgroundColor: W.bg,
  border: `1px solid ${focused ? W.accent : W.input}`,
  outline: focused ? `1px solid ${W.accent}` : "none",
  borderRadius: 8,
  padding: "8px 10px",
})

const Caret: React.FC<{ on: boolean }> = ({ on }) => (
  <span
    style={{
      display: "inline-block",
      width: 1.5,
      height: 16,
      marginLeft: 1,
      verticalAlign: "text-bottom",
      backgroundColor: W.fg,
      opacity: on ? 1 : 0,
    }}
  />
)

// The screenshot as the widget shows it: the page raster, contained in the
// 180px-tall thumb (object-fit: contain) — plus the flattened rect.
const Shot: React.FC<{ width: number; height: number; marked: boolean }> = ({ width, height, marked }) => {
  const s = Math.min(width / WIN.w, height / PAGE_H)
  const w = WIN.w * s
  const h = PAGE_H * s
  return (
    <div style={{ position: "relative", width, height, overflow: "hidden" }}>
      <div style={{ position: "absolute", left: (width - w) / 2, top: (height - h) / 2, width: w, height: h, overflow: "hidden" }}>
        <div style={{ position: "absolute", left: 0, top: 0, width: WIN.w, height: PAGE_H, scale: String(s), transformOrigin: "0 0" }}>
          <CheckoutPage frame={0} />
          {marked ? (
            <div
              style={{
                position: "absolute",
                left: MARK.x,
                top: MARK.y,
                width: MARK.w,
                height: MARK.h,
                boxSizing: "border-box",
                border: `${MARK_STROKE}px solid ${W.destructive}`,
              }}
            />
          ) : null}
        </div>
      </div>
    </div>
  )
}

// ── The panel ─────────────────────────────────────────────────────────────────
export type WidgetPanelProps = {
  frame: number
  appearAt: number // the launcher click mounts the panel (plain, no shot)
  captureAt?: number // Take screenshot → the annotator opens
  markFrom?: number // the rect drag around the dead Pay button
  markTo?: number
  doneAt?: number // annotator Done → back to the panel with the thumbnail
  messageFocusAt?: number
  messageTypeAt?: number // the one message types (3 chars/frame)
  sendHoverAt?: number
  sendingAt?: number // button → "Sending…" (disabled, 0.55)
  successAt?: number // the success view replaces the form
}

export const WidgetPanel: React.FC<WidgetPanelProps> = ({
  frame,
  appearAt,
  captureAt,
  markFrom,
  markTo,
  doneAt,
  messageFocusAt,
  messageTypeAt,
  sendHoverAt,
  sendingAt,
  successAt,
}) => {
  const blinkOn = useBlink(frame)
  if (frame < appearAt) return null

  const appearO = interpolate(frame, [appearAt, appearAt + 4], [0, 1], CLAMP)
  const annotating = captureAt !== undefined && frame >= captureAt && (doneAt === undefined || frame < doneAt)
  const hasShot = doneAt !== undefined && frame >= doneAt
  const succeeded = successAt !== undefined && frame >= successAt

  // ── the annotator overlay ──
  if (annotating && captureAt !== undefined) {
    const o = interpolate(frame, [captureAt, captureAt + 4], [0, 1], CLAMP)
    const drawT = markFrom === undefined || markTo === undefined ? 0 : interpolate(frame, [markFrom, markTo], [0, 1], EASED)
    const drawing = markFrom !== undefined && frame >= markFrom
    return (
      <div
        style={{
          position: "absolute",
          left: 0,
          top: CHROME_H,
          width: WIN.w,
          height: PAGE_H,
          backgroundColor: "rgba(0,0,0,0.8)",
          fontFamily: UI_FONT,
          color: W.fg,
          opacity: o,
          zIndex: 40,
        }}
      >
        {/* .exp-ann-toolbar */}
        <div
          style={{
            position: "absolute",
            left: ANN_TOOLBAR_X,
            top: ANN_TOOLBAR_TOP,
            width: ANN_TOOLBAR_W,
            height: ANN_TOOLBAR_H,
            boxSizing: "border-box",
            display: "flex",
            alignItems: "center",
            gap: 4,
            padding: 6,
            backgroundColor: W.card,
            border: `1px solid ${W.border}`,
            borderRadius: 12,
            boxShadow: "0 8px 28px rgba(0,0,0,0.5)",
          }}
        >
          {ANN_ICONS.map((icon, i) => (
            <span
              key={i}
              style={{
                width: 32,
                height: 32,
                borderRadius: 8,
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                // rect is the default tool (aria-pressed)
                backgroundColor: i === 0 ? W.secondary : "transparent",
                color: i === 0 ? W.destructive : W.muted,
              }}
            >
              <Svg size={16}>{icon}</Svg>
            </span>
          ))}
          <span style={{ width: 1, height: 20, margin: "0 4px", backgroundColor: W.border }} />
          {ANN_ACTION_ICONS.map((icon, i) => (
            <span
              key={i}
              style={{
                width: 32,
                height: 32,
                borderRadius: 8,
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                color: W.muted,
                // undo/clear enable once a shape exists; remove-crop stays off
                opacity: i < 2 && drawing ? 1 : 0.4,
              }}
            >
              <Svg size={16}>{icon}</Svg>
            </span>
          ))}
          <span style={{ width: 1, height: 20, margin: "0 4px", backgroundColor: W.border }} />
          <span
            style={{
              width: ANN_CANCEL_W,
              boxSizing: "border-box",
              textAlign: "center",
              fontSize: 12.5,
              fontWeight: 600,
              padding: "7px 0",
              borderRadius: 8,
              color: W.muted,
            }}
          >
            Cancel
          </span>
          <span
            style={{
              width: ANN_DONE_W,
              boxSizing: "border-box",
              textAlign: "center",
              fontSize: 12.5,
              fontWeight: 600,
              padding: "7px 0",
              borderRadius: 8,
              backgroundColor: W.accent,
              color: W.accentFg,
            }}
          >
            Done
          </span>
        </div>

        {/* .exp-ann-stage canvas: the page raster + the live rect */}
        <div
          style={{
            position: "absolute",
            left: ANN_LEFT,
            top: ANN_TOP,
            width: ANN_W,
            height: ANN_H,
            border: `1px solid ${W.border}`,
            borderRadius: 8,
            boxShadow: "0 12px 40px rgba(0,0,0,0.6)",
            overflow: "hidden",
            // the canvas is fully covered by the raster (widget.css keeps a
            // #fff fallback that never shows through a loaded screenshot)
            backgroundColor: "#0d0d10",
          }}
        >
          <div style={{ position: "absolute", left: 0, top: 0, width: WIN.w, height: PAGE_H, scale: String(ANN_SCALE), transformOrigin: "0 0" }}>
            <CheckoutPage frame={0} />
            {drawing ? (
              <div
                style={{
                  position: "absolute",
                  left: MARK.x,
                  top: MARK.y,
                  width: Math.max(MARK_STROKE, MARK.w * drawT),
                  height: Math.max(MARK_STROKE, MARK.h * drawT),
                  boxSizing: "border-box",
                  border: `${MARK_STROKE}px solid ${W.destructive}`,
                }}
              />
            ) : null}
          </div>
        </div>
      </div>
    )
  }

  // ── success view (Panel.tsx phase "success": no header) ──
  if (succeeded) {
    return (
      <div
        style={{
          position: "absolute",
          right: PANEL_RIGHT,
          bottom: PANEL_BOTTOM,
          width: PANEL_W,
          boxSizing: "border-box",
          display: "flex",
          flexDirection: "column",
          backgroundColor: W.card,
          border: `1px solid ${W.border}`,
          borderRadius: 10,
          boxShadow: "0 12px 40px rgba(0,0,0,0.55)",
          overflow: "hidden",
          fontFamily: UI_FONT,
          fontSize: 14,
          lineHeight: 1.45,
          color: W.fg,
          zIndex: 30,
        }}
      >
        <div
          style={{
            padding: "36px 20px",
            display: "flex",
            flexDirection: "column",
            alignItems: "center",
            gap: 10,
            textAlign: "center",
          }}
        >
          <div
            style={{
              width: 36,
              height: 36,
              borderRadius: 999,
              backgroundColor: "rgba(34,197,94,0.15)",
              color: W.success,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
            }}
          >
            <CheckIcon />
          </div>
          <div style={{ fontSize: 14, fontWeight: 600 }}>{WIDGET_COPY.successTitle}</div>
          <div style={{ fontSize: 12.5, color: W.muted }}>{WIDGET_COPY.successSub}</div>
        </div>
        <Powered />
      </div>
    )
  }

  // ── the form ──
  const messageText = messageTypeAt === undefined ? "" : typed(WIDGET_COPY.message, frame, messageTypeAt, 3)
  const messageFocused = messageFocusAt !== undefined && frame >= messageFocusAt && (sendingAt === undefined || frame < sendingAt)
  const sending = sendingAt !== undefined && frame >= sendingAt
  const hoverT = sendHoverAt === undefined ? 0 : interpolate(frame, [sendHoverAt, sendHoverAt + 4], [0, 1], CLAMP)

  return (
    <div
      style={{
        position: "absolute",
        right: PANEL_RIGHT,
        bottom: PANEL_BOTTOM,
        width: PANEL_W,
        boxSizing: "border-box",
        display: "flex",
        flexDirection: "column",
        backgroundColor: W.card,
        border: `1px solid ${W.border}`,
        borderRadius: 10,
        boxShadow: "0 12px 40px rgba(0,0,0,0.55)",
        overflow: "hidden",
        fontFamily: UI_FONT,
        fontSize: 14,
        lineHeight: 1.45,
        color: W.fg,
        opacity: appearO,
        zIndex: 30,
      }}
    >
      {/* .exp-header */}
      <div
        style={{
          height: HEADER_H,
          boxSizing: "border-box",
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          padding: "12px 16px",
          borderBottom: `1px solid ${W.border}`,
        }}
      >
        <span style={{ fontSize: 14, fontWeight: 600 }}>{WIDGET_COPY.panelTitle}</span>
        <span style={{ width: 26, height: 26, display: "flex", alignItems: "center", justifyContent: "center", color: W.muted }}>
          <CloseIcon />
        </span>
      </div>

      {/* .exp-body */}
      <div style={{ padding: `${BODY_PAD_Y}px 16px`, display: "flex", flexDirection: "column", gap: BODY_GAP }}>
        {/* .exp-shot */}
        <div
          style={{
            boxSizing: "border-box",
            height: hasShot ? SHOT_FULL_H : SHOT_EMPTY_H,
            border: `1px solid ${W.border}`,
            borderRadius: 8,
            backgroundColor: W.bg,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            overflow: "hidden",
            position: "relative",
          }}
        >
          {hasShot ? (
            <>
              <Shot width={INNER_W - 2} height={SHOT_IMG_H} marked />
              <div style={{ position: "absolute", top: 6, right: 6, display: "flex", gap: 6 }}>
                {["Annotate", "Retake", "Remove"].map((chip) => (
                  <span key={chip} style={chipStyle}>
                    {chip}
                  </span>
                ))}
              </div>
            </>
          ) : (
            <div
              style={{
                padding: 16,
                textAlign: "center",
                color: W.muted,
                fontSize: 12.5,
                display: "flex",
                flexDirection: "column",
                gap: 8,
                alignItems: "center",
              }}
            >
              <span>Attach a screenshot of this page.</span>
              <div style={{ display: "inline-flex" }}>
                <span style={{ ...chipStyle, width: TAKE_W, boxSizing: "border-box", textAlign: "center", borderTopRightRadius: 0, borderBottomRightRadius: 0 }}>
                  Take screenshot
                </span>
                <span
                  style={{
                    ...chipStyle,
                    display: "inline-flex",
                    alignItems: "center",
                    gap: 3,
                    padding: "4px 6px",
                    borderTopLeftRadius: 0,
                    borderBottomLeftRadius: 0,
                    borderLeft: 0,
                    color: "rgba(250,250,250,0.75)",
                  }}
                >
                  <TimerIcon />
                  Off
                </span>
              </div>
            </div>
          )}
        </div>

        {/* .exp-images — the dashed Add image tile (left-aligned row) */}
        <div style={{ display: "flex", height: ADD_IMAGE_H }}>
          <span
            style={{
              boxSizing: "border-box",
              height: ADD_IMAGE_H,
              padding: "4px 10px",
              border: `1px dashed ${W.border}`,
              borderRadius: 8,
              color: W.muted,
              fontSize: 11.5,
              fontWeight: 500,
              display: "flex",
              alignItems: "center",
            }}
          >
            {WIDGET_COPY.addImage}
          </span>
        </div>

        {/* What happened? */}
        <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
          <Label>{WIDGET_COPY.messageLabel}</Label>
          <div style={{ ...fieldStyle(messageFocused), height: TEXTAREA_H, whiteSpace: "pre-wrap", overflow: "hidden" }}>
            {messageText.length > 0 ? messageText : null}
            {messageFocused ? <Caret on={blinkOn || messageText.length < WIDGET_COPY.message.length} /> : null}
            {messageText.length === 0 ? (
              <span style={{ color: W.muted, opacity: 0.7 }}>Describe what you saw, or what you'd like to see.</span>
            ) : null}
          </div>
        </div>

        {/* Email (optional) — collectEmail defaults on */}
        <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
          <Label>{WIDGET_COPY.emailLabel}</Label>
          <div style={{ ...fieldStyle(false), height: INPUT_H }}>
            <span style={{ color: W.muted, opacity: 0.7 }}>{WIDGET_COPY.emailPlaceholder}</span>
          </div>
        </div>

        {/* .exp-footer — right-aligned submit */}
        <div style={{ display: "flex", justifyContent: "flex-end" }}>
          <span
            style={{
              width: SUBMIT_W,
              height: SUBMIT_H,
              boxSizing: "border-box",
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              fontSize: 13,
              fontWeight: 600,
              borderRadius: 8,
              backgroundColor: W.accent,
              color: W.accentFg,
              filter: hoverT > 0 && !sending ? `brightness(${1 + 0.08 * hoverT})` : undefined,
              opacity: sending ? 0.55 : 1,
            }}
          >
            {sending ? WIDGET_COPY.sending : WIDGET_COPY.send}
          </span>
        </div>
      </div>

      <Powered />
    </div>
  )
}

// .exp-powered — its own strip with a top hairline.
const Powered: React.FC = () => (
  <div
    style={{
      height: POWERED_H,
      boxSizing: "border-box",
      padding: "7px 16px",
      borderTop: `1px solid ${W.border}`,
      textAlign: "center",
      fontSize: 11,
      lineHeight: 1.45,
      color: W.muted,
    }}
  >
    Powered by <span style={{ fontWeight: 600 }}>Exponential</span>
  </div>
)

// Window-local rect of the form panel once the thumbnail is in (for cameras).
export const PANEL_RECT = {
  x: PANEL_X,
  y: TOP_SHOT,
  w: PANEL_W,
  h: PANEL_BOTTOM_Y - TOP_SHOT,
} as const
export const PANEL_RECT_EMPTY_TOP = TOP_EMPTY
