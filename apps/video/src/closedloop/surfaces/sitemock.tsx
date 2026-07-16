// closedloop/surfaces/sitemock.tsx — the LIGHT-MODE third-party site: a browser
// chassis at the shared WIN comp coords (so the ships Camera/CursorLayer rigs work
// unchanged), the acme.shop checkout page (PAGE-local coords, reusable scaled-down
// as the widget screenshot thumbnail), the floating feedback FAB (modeled on
// packages/widget/src/theme.ts — 44px pill, accent #e5e5e5, megaphone icon) and
// the closing-beat email card. All frame props are COMPOSITION-GLOBAL.

import React from "react"
import { interpolate, spring } from "remotion"
import { EASE, SETTLE, UI_FONT, WIN } from "../../ships/theme"
import { CL, EMAIL, SITE } from "../fixtures"

const CLAMP = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const
const EASED = { ...CLAMP, easing: EASE } as const

// Light third-party palette — deliberately foreign to the app theme (the site
// must instantly read as NOT our app).
const S = {
  chrome: "#e8eaed",
  chromeBorder: "#d4d7dc",
  urlBg: "#ffffff",
  urlText: "#3c4043",
  page: "#f4f5f7",
  surface: "#ffffff",
  border: "#e4e7ec",
  text: "#111827",
  muted: "#6b7280",
  faint: "#9aa1ac",
  inputBorder: "#d8dce3",
  dark: "#111827",
} as const

// Widget-launcher tokens (packages/widget/src/theme.ts).
const FAB_ACCENT = "#e5e5e5"
const FAB_FG = "#171717"

// ── Geometry (window-local; the page area sits below the 44px browser chrome) ──
export const CHROME_H = 44
export const PAGE_H = WIN.h - CHROME_H // 936

// PAGE-local layout constants.
const HEADER_H = 64
const COL_L = 204
const COL_L_W = 620
const CARD_X = 864
const CARD_W = 500
const CARD_PAD = 24
const CARD_Y = 96
const PAY = { x: CARD_X + CARD_PAD, y: 380, w: CARD_W - 2 * CARD_PAD, h: 46 } // page-local

// Window-local cursor/annotation anchors.
export const SITE_ANCHORS = {
  payButton: { x: PAY.x + PAY.w / 2, y: CHROME_H + PAY.y + PAY.h / 2 }, // (1114, 447)
  fab: { x: WIN.w - 20 - 22, y: WIN.h - 20 - 22 }, // (1526, 938)
  payRectPage: PAY, // page-local rect (for the widget-thumbnail annotation)
} as const

// ── Tiny glyphs ───────────────────────────────────────────────────────────────
const Svg: React.FC<{ size: number; sw?: number; children: React.ReactNode }> = ({ size, sw = 1.8, children }) => (
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

const LockIcon: React.FC<{ size?: number }> = ({ size = 11 }) => (
  <Svg size={size} sw={2}>
    <rect x="4" y="11" width="16" height="10" rx="2" />
    <path d="M8 11V7a4 4 0 0 1 8 0v4" />
  </Svg>
)

const SearchIcon: React.FC<{ size?: number }> = ({ size = 15 }) => (
  <Svg size={size}>
    <circle cx="11" cy="11" r="8" />
    <path d="m21 21-4.3-4.3" />
  </Svg>
)

const BagIcon: React.FC<{ size?: number }> = ({ size = 15 }) => (
  <Svg size={size}>
    <path d="M6 7h12l1 14H5Z" />
    <path d="M9 10V6a3 3 0 0 1 6 0v4" />
  </Svg>
)

const CardIcon: React.FC<{ size?: number }> = ({ size = 15 }) => (
  <Svg size={size} sw={1.7}>
    <rect x="2" y="5" width="20" height="14" rx="2" />
    <path d="M2 10h20" />
  </Svg>
)

const MegaphoneIcon: React.FC<{ size?: number }> = ({ size = 16 }) => (
  <Svg size={size} sw={2}>
    <path d="m3 11 18-5v12L3 14v-3z" />
    <path d="M11.6 16.8a3 3 0 1 1-5.8-1.6" />
  </Svg>
)

const MailIcon: React.FC<{ size?: number }> = ({ size = 14 }) => (
  <Svg size={size} sw={1.8}>
    <rect x="2" y="4" width="20" height="16" rx="2" />
    <path d="m22 7-8.97 5.7a1.94 1.94 0 0 1-2.06 0L2 7" />
  </Svg>
)

const CheckIcon: React.FC<{ size?: number }> = ({ size = 12 }) => (
  <Svg size={size} sw={2.6}>
    <path d="M20 6 9 17l-5-5" />
  </Svg>
)

// ── Browser chassis (comp coords WIN.x/WIN.y — camera-rig compatible) ─────────
export const BrowserChassis: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <div
    style={{
      position: "absolute",
      left: WIN.x,
      top: WIN.y,
      width: WIN.w,
      height: WIN.h,
      borderRadius: WIN.radius,
      boxShadow: "0 40px 120px rgba(0,0,0,0.6), 0 8px 24px rgba(0,0,0,0.4)",
      backgroundColor: S.page,
      overflow: "hidden",
      fontFamily: UI_FONT,
    }}
  >
    {/* browser chrome strip */}
    <div
      style={{
        position: "absolute",
        top: 0,
        left: 0,
        right: 0,
        height: CHROME_H,
        boxSizing: "border-box",
        backgroundColor: S.chrome,
        borderBottom: `1px solid ${S.chromeBorder}`,
        display: "flex",
        alignItems: "center",
      }}
    >
      {/* traffic lights */}
      <div style={{ display: "flex", gap: 8, paddingLeft: 18 }}>
        <span style={{ width: 11, height: 11, borderRadius: 999, backgroundColor: "#ff5f57" }} />
        <span style={{ width: 11, height: 11, borderRadius: 999, backgroundColor: "#febc2e" }} />
        <span style={{ width: 11, height: 11, borderRadius: 999, backgroundColor: "#28c840" }} />
      </div>
      {/* URL pill */}
      <div
        style={{
          position: "absolute",
          left: (WIN.w - 480) / 2,
          top: 8,
          width: 480,
          height: 28,
          boxSizing: "border-box",
          borderRadius: 14,
          backgroundColor: S.urlBg,
          border: `1px solid ${S.chromeBorder}`,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          gap: 6,
          color: S.urlText,
        }}
      >
        <LockIcon size={11} />
        <span style={{ fontSize: 12.5 }}>{CL.siteUrl}</span>
      </div>
    </div>
    {children}
  </div>
)

// ── Shared field primitives (page-local absolute) ─────────────────────────────
const FieldLabel: React.FC<{ x: number; y: number; children: React.ReactNode }> = ({ x, y, children }) => (
  <div style={{ position: "absolute", left: x, top: y, fontSize: 13, fontWeight: 600, color: S.text, letterSpacing: 0.2 }}>
    {children}
  </div>
)

const Field: React.FC<{ x: number; y: number; w: number; value: string; icon?: React.ReactNode; muted?: boolean }> = ({
  x,
  y,
  w,
  value,
  icon,
  muted = false,
}) => (
  <div
    style={{
      position: "absolute",
      left: x,
      top: y,
      width: w,
      height: 42,
      boxSizing: "border-box",
      borderRadius: 8,
      border: `1px solid ${S.inputBorder}`,
      backgroundColor: S.surface,
      display: "flex",
      alignItems: "center",
      gap: 8,
      padding: "0 12px",
      color: S.muted,
    }}
  >
    {icon ?? null}
    <span style={{ fontSize: 13.5, color: muted ? S.faint : S.text }}>{value}</span>
  </div>
)

// ── The checkout page (PAGE-local coords, 1568×936) ───────────────────────────
// `shakeAts` = global frames of the dead Pay-now clicks (micro-shake ±4px, 8f).
export const CheckoutPage: React.FC<{ frame: number; shakeAts?: readonly number[] }> = ({ frame, shakeAts = [] }) => {
  let shakeX = 0
  for (const at of shakeAts) {
    shakeX += interpolate(frame, [at, at + 2, at + 4, at + 6, at + 8], [0, -4, 4, -2, 0], CLAMP)
  }
  return (
    <div style={{ position: "absolute", inset: 0, backgroundColor: S.page, fontFamily: UI_FONT }}>
      {/* site header */}
      <div
        style={{
          position: "absolute",
          top: 0,
          left: 0,
          right: 0,
          height: HEADER_H,
          boxSizing: "border-box",
          backgroundColor: S.surface,
          borderBottom: `1px solid ${S.border}`,
        }}
      >
        <div style={{ position: "absolute", left: COL_L, top: 0, height: HEADER_H, display: "flex", alignItems: "center", gap: 10 }}>
          <div
            style={{
              width: 26,
              height: 26,
              borderRadius: 7,
              backgroundColor: S.dark,
              color: "#ffffff",
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              fontSize: 14,
              fontWeight: 800,
            }}
          >
            A
          </div>
          <span style={{ fontSize: 15, fontWeight: 800, letterSpacing: 2, color: S.text }}>{CL.brand}</span>
          <div style={{ display: "flex", gap: 26, marginLeft: 36 }}>
            {SITE.nav.map((n) => (
              <span key={n} style={{ fontSize: 13.5, color: S.muted }}>
                {n}
              </span>
            ))}
          </div>
        </div>
        <div
          style={{
            position: "absolute",
            right: WIN.w - (CARD_X + CARD_W),
            top: 0,
            height: HEADER_H,
            display: "flex",
            alignItems: "center",
            gap: 20,
            color: S.muted,
          }}
        >
          <SearchIcon size={15} />
          <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
            <BagIcon size={15} />
            <span style={{ fontSize: 13.5 }}>{SITE.cart}</span>
          </div>
        </div>
      </div>

      {/* page title */}
      <div style={{ position: "absolute", left: COL_L, top: 96, fontSize: 26, fontWeight: 700, letterSpacing: -0.4, color: S.text }}>
        Checkout
      </div>

      {/* left column — contact / shipping / payment */}
      <FieldLabel x={COL_L} y={152}>{SITE.contactLabel}</FieldLabel>
      <Field x={COL_L} y={174} w={COL_L_W} value={SITE.email} />

      <FieldLabel x={COL_L} y={244}>{SITE.shippingLabel}</FieldLabel>
      <Field x={COL_L} y={266} w={COL_L_W} value={SITE.name} />
      <Field x={COL_L} y={318} w={COL_L_W} value={SITE.address} />
      <Field x={COL_L} y={370} w={COL_L_W} value={SITE.cityRow} />

      <FieldLabel x={COL_L} y={440}>{SITE.paymentLabel}</FieldLabel>
      <Field x={COL_L} y={462} w={COL_L_W} value={SITE.card} icon={<CardIcon size={15} />} />
      <Field x={COL_L} y={514} w={(COL_L_W - 12) / 2} value={SITE.expiry} muted />
      <Field x={COL_L + (COL_L_W - 12) / 2 + 12} y={514} w={(COL_L_W - 12) / 2} value={SITE.cvc} muted />

      {/* right column — order summary card */}
      <div
        style={{
          position: "absolute",
          left: CARD_X,
          top: CARD_Y,
          width: CARD_W,
          height: 478 - CARD_Y,
          boxSizing: "border-box",
          borderRadius: 12,
          border: `1px solid ${S.border}`,
          backgroundColor: S.surface,
          boxShadow: "0 8px 28px rgba(17,24,39,0.06)",
        }}
      />
      <div style={{ position: "absolute", left: CARD_X + CARD_PAD, top: 120, fontSize: 15, fontWeight: 700, color: S.text }}>
        {SITE.summaryLabel}
      </div>
      {SITE.items.map((item, i) => (
        <div
          key={item.name}
          style={{
            position: "absolute",
            left: CARD_X + CARD_PAD,
            top: 152 + 56 * i,
            width: CARD_W - 2 * CARD_PAD,
            height: 56,
            display: "flex",
            alignItems: "center",
            gap: 12,
          }}
        >
          <div style={{ width: 42, height: 42, borderRadius: 8, backgroundColor: item.tint, opacity: 0.55, flexShrink: 0 }} />
          <div style={{ flex: 1, minWidth: 0 }}>
            <div style={{ fontSize: 13.5, fontWeight: 600, color: S.text }}>{item.name}</div>
            <div style={{ fontSize: 12, color: S.muted, marginTop: 2 }}>{item.variant}</div>
          </div>
          <div style={{ fontSize: 13.5, fontWeight: 600, color: S.text }}>{item.price}</div>
        </div>
      ))}
      <div style={{ position: "absolute", left: CARD_X + CARD_PAD, top: 272, width: CARD_W - 2 * CARD_PAD, height: 1, backgroundColor: S.border }} />
      {[SITE.subtotal, SITE.shipping].map(([label, value], i) => (
        <div
          key={label}
          style={{
            position: "absolute",
            left: CARD_X + CARD_PAD,
            top: 284 + 24 * i,
            width: CARD_W - 2 * CARD_PAD,
            display: "flex",
            justifyContent: "space-between",
            fontSize: 13,
            color: S.muted,
          }}
        >
          <span>{label}</span>
          <span>{value}</span>
        </div>
      ))}
      <div
        style={{
          position: "absolute",
          left: CARD_X + CARD_PAD,
          top: 340,
          width: CARD_W - 2 * CARD_PAD,
          display: "flex",
          justifyContent: "space-between",
          fontSize: 15,
          fontWeight: 700,
          color: S.text,
        }}
      >
        <span>{SITE.total[0]}</span>
        <span>{SITE.total[1]}</span>
      </div>

      {/* THE dead Pay-now button */}
      <div
        style={{
          position: "absolute",
          left: PAY.x,
          top: PAY.y,
          width: PAY.w,
          height: PAY.h,
          borderRadius: 8,
          backgroundColor: S.dark,
          color: "#ffffff",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          fontSize: 14.5,
          fontWeight: 600,
          translate: `${shakeX}px 0px`,
        }}
      >
        {SITE.payLabel}
      </div>
      <div
        style={{
          position: "absolute",
          left: PAY.x,
          top: 438,
          width: PAY.w,
          textAlign: "center",
          fontSize: 12,
          color: S.faint,
        }}
      >
        {SITE.secure}
      </div>
    </div>
  )
}

// Convenience wrapper: the page mounted below the browser chrome (window-local).
export const SiteViewport: React.FC<{ frame: number; shakeAts?: readonly number[] }> = ({ frame, shakeAts }) => (
  <div style={{ position: "absolute", left: 0, top: CHROME_H, width: WIN.w, height: PAGE_H, overflow: "hidden" }}>
    <CheckoutPage frame={frame} shakeAts={shakeAts} />
  </div>
)

// ── The floating feedback FAB (widget launcher) ───────────────────────────────
// Icon-only 44px circle at rest; `hoverAt` grows it into the labeled pill (the
// real launcher's hover reveal); `pressAt` = click dip; `restAt` collapses back.
export const FeedbackFab: React.FC<{ frame: number; hoverAt?: number; pressAt?: number; restAt?: number }> = ({
  frame,
  hoverAt,
  pressAt,
  restAt,
}) => {
  const grow =
    hoverAt === undefined
      ? 0
      : interpolate(frame, [hoverAt, hoverAt + 8], [0, 1], EASED) *
        (restAt === undefined ? 1 : 1 - interpolate(frame, [restAt, restAt + 8], [0, 1], EASED))
  const w = 44 + 78 * grow
  const press = pressAt === undefined ? 1 : interpolate(frame, [pressAt, pressAt + 2, pressAt + 5], [1, 0.9, 1], CLAMP)
  const scale = (1 + 0.08 * grow) * press
  return (
    <div
      style={{
        position: "absolute",
        right: 20,
        bottom: 20,
        width: w,
        height: 44,
        boxSizing: "border-box",
        borderRadius: 999,
        border: `1px solid rgba(255,255,255,0.1)`,
        backgroundColor: FAB_ACCENT,
        color: FAB_FG,
        boxShadow: `0 4px 16px rgba(0,0,0,${0.4 + 0.1 * grow})`,
        display: "flex",
        alignItems: "center",
        justifyContent: grow > 0.02 ? "flex-start" : "center",
        paddingLeft: grow > 0.02 ? 13 : 0,
        gap: 7,
        overflow: "hidden",
        scale: String(scale),
        transformOrigin: "center",
        fontFamily: UI_FONT,
      }}
    >
      <MegaphoneIcon size={16} />
      <span style={{ fontSize: 13, fontWeight: 600, whiteSpace: "nowrap", opacity: grow }}>Feedback</span>
    </div>
  )
}

// ── The closing email card (window-local) ─────────────────────────────────────
export const EmailCard: React.FC<{ frame: number; appearAt: number; fadeAt?: number }> = ({ frame, appearAt, fadeAt }) => {
  if (frame < appearAt) return null
  const pop = spring({ frame: frame - appearAt, fps: 30, config: SETTLE })
  const fade = fadeAt === undefined ? 1 : 1 - interpolate(frame, [fadeAt, fadeAt + 12], [0, 1], EASED)
  const o = interpolate(frame, [appearAt, appearAt + 8], [0, 1], CLAMP) * fade
  if (o <= 0) return null
  return (
    <div
      style={{
        position: "absolute",
        left: 560,
        top: 336,
        width: 480,
        boxSizing: "border-box",
        borderRadius: 12,
        border: `1px solid ${S.border}`,
        backgroundColor: S.surface,
        boxShadow: "0 24px 64px rgba(17,24,39,0.22), 0 4px 16px rgba(17,24,39,0.10)",
        padding: 20,
        opacity: o,
        translate: `0px ${26 * (1 - pop)}px`,
        fontFamily: UI_FONT,
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
        <div
          style={{
            width: 28,
            height: 28,
            borderRadius: 999,
            backgroundColor: S.dark,
            color: "#ffffff",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            flexShrink: 0,
          }}
        >
          <MailIcon size={14} />
        </div>
        <span style={{ fontSize: 12.5, color: S.muted, flex: 1 }}>{EMAIL.from}</span>
        <span style={{ fontSize: 12, color: S.faint }}>{EMAIL.time}</span>
      </div>
      <div style={{ marginTop: 12, fontSize: 15.5, fontWeight: 700, color: S.text, letterSpacing: -0.2 }}>{EMAIL.subject}</div>
      <div style={{ marginTop: 6, fontSize: 13.5, lineHeight: 1.55, color: "#374151" }}>{EMAIL.body}</div>
      <div
        style={{
          marginTop: 12,
          display: "inline-flex",
          alignItems: "center",
          gap: 6,
          height: 22,
          padding: "0 9px",
          borderRadius: 999,
          backgroundColor: "rgba(34,197,94,0.12)",
          color: "#15803d",
          fontSize: 12,
          fontWeight: 600,
        }}
      >
        <CheckIcon size={11} />
        Resolved
      </div>
    </div>
  )
}
