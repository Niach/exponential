// showreel/atoms.tsx — the reel's vocabulary of app parts (EXP-1100): glass
// surfaces, the FOUR status glyphs (registry geometry: circle-dashed /
// progress-2-4 / progress-3-4 / circle-check, status-icon-svg.ts), identifier
// chips, avatars, priority bars, an issue row, the live dot, ripples, kinetic
// type, the act caption and the cursor.

import React from "react"
import { AVATAR, C, DISPLAY, MONO, UI, VIOLET } from "./theme"
import { IN, enter, seg } from "./motion"

// ── Surfaces ────────────────────────────────────────────────────────────────
export const Glass: React.FC<{
  x: number
  y: number
  w: number
  h: number
  r?: number
  fill?: string
  stroke?: string
  style?: React.CSSProperties
  children?: React.ReactNode
}> = ({ x, y, w, h, r = 16, fill = C.fillCard, stroke = C.strokeCard, style, children }) => (
  <div
    style={{
      position: `absolute`,
      left: x,
      top: y,
      width: w,
      height: h,
      borderRadius: r,
      background: `linear-gradient(to bottom, rgba(17,17,20,0.92), rgba(12,12,15,0.92))`,
      boxShadow: `0 30px 80px -20px rgba(0,0,0,0.7), inset 0 1px 0 rgba(255,255,255,0.07)`,
      overflow: `hidden`,
      ...style,
    }}
  >
    <div style={{ position: `absolute`, inset: 0, background: fill, borderRadius: r }} />
    <div
      style={{
        position: `absolute`,
        inset: 0,
        borderRadius: r,
        border: `1px solid ${stroke}`,
        pointerEvents: `none`,
      }}
    />
    {children}
  </div>
)

// ── Status glyphs ───────────────────────────────────────────────────────────
export type StatusKind = `backlog` | `progress` | `review` | `done`

export const STATUS_COLOR: Record<StatusKind, string> = {
  backlog: C.statusBacklog,
  progress: C.statusInProgress,
  review: C.statusInReview,
  done: C.statusDone,
}

const DASHED = [
  `M10.1 2.182a10 10 0 0 1 3.8 0`,
  `M13.9 21.818a10 10 0 0 1-3.8 0`,
  `M17.609 3.721a10 10 0 0 1 2.69 2.7`,
  `M2.182 13.9a10 10 0 0 1 0-3.8`,
  `M20.279 17.609a10 10 0 0 1-2.7 2.69`,
  `M21.818 10.1a10 10 0 0 1 0 3.8`,
  `M3.721 6.391a10 10 0 0 1 2.7-2.69`,
  `M6.391 20.279a10 10 0 0 1-2.69-2.7`,
]

export const StatusGlyph: React.FC<{
  kind: StatusKind
  size: number
  color?: string
  style?: React.CSSProperties
}> = ({ kind, size, color, style }) => {
  const c = color ?? STATUS_COLOR[kind]
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke={c}
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      style={{ display: `block`, flexShrink: 0, ...style }}
    >
      {kind === `backlog` ? (
        DASHED.map((d) => <path key={d} d={d} />)
      ) : (
        <circle cx={12} cy={12} r={10} />
      )}
      {kind === `progress` ? (
        <path d="M12 12 L12 6 A6 6 0 0 1 12 18 Z" fill={c} stroke="none" />
      ) : null}
      {kind === `review` ? (
        <path d="M12 12 L12 6 A6 6 0 1 1 6 12 Z" fill={c} stroke="none" />
      ) : null}
      {kind === `done` ? <path d="m9 12 2 2 4-4" /> : null}
    </svg>
  )
}

// ── Chips ───────────────────────────────────────────────────────────────────
export const Ident: React.FC<{
  text: string
  size?: number
  color?: string
  style?: React.CSSProperties
}> = ({ text, size = 18, color = C.dim, style }) => (
  <span
    style={{
      fontFamily: MONO,
      fontSize: size,
      fontWeight: 500,
      color,
      letterSpacing: `-0.01em`,
      whiteSpace: `nowrap`,
      flexShrink: 0,
      ...style,
    }}
  >
    {text}
  </span>
)

export const Pill: React.FC<{
  size?: number
  color?: string
  fill?: string
  stroke?: string
  style?: React.CSSProperties
  children: React.ReactNode
}> = ({ size = 15, color = C.text, fill = C.fillActive, stroke = C.strokeActive, style, children }) => (
  <span
    style={{
      display: `inline-flex`,
      alignItems: `center`,
      gap: size * 0.45,
      padding: `${size * 0.28}px ${size * 0.7}px`,
      borderRadius: 999,
      background: fill,
      border: `1px solid ${stroke}`,
      fontFamily: UI,
      fontSize: size,
      fontWeight: 500,
      color,
      whiteSpace: `nowrap`,
      lineHeight: 1.2,
      ...style,
    }}
  >
    {children}
  </span>
)

export const Avatar: React.FC<{
  initials: string
  hue: number
  size: number
  style?: React.CSSProperties
}> = ({ initials, hue, size, style }) => (
  <span
    style={{
      display: `inline-flex`,
      alignItems: `center`,
      justifyContent: `center`,
      width: size,
      height: size,
      borderRadius: `50%`,
      background: AVATAR[hue % AVATAR.length],
      color: `#0a0a0a`,
      fontFamily: UI,
      fontWeight: 600,
      fontSize: size * 0.42,
      letterSpacing: `-0.02em`,
      flexShrink: 0,
      ...style,
    }}
  >
    {initials}
  </span>
)

export const PrioBars: React.FC<{ level: 0 | 1 | 2 | 3; size?: number }> = ({
  level,
  size = 16,
}) => (
  <span
    style={{
      display: `inline-flex`,
      alignItems: `flex-end`,
      gap: size * 0.14,
      height: size,
      flexShrink: 0,
    }}
  >
    {[0.45, 0.72, 1].map((h, i) => (
      <span
        key={h}
        style={{
          width: size * 0.2,
          height: size * h,
          borderRadius: 1.5,
          background: i < level ? C.muted : C.strokeStrong,
        }}
      />
    ))}
  </span>
)

// ── Issue row ───────────────────────────────────────────────────────────────
export type Person = { initials: string; hue: number }

export const IssueRow: React.FC<{
  ident: string
  title: React.ReactNode
  status: StatusKind
  prio?: 0 | 1 | 2 | 3
  assignee?: Person | null
  h?: number
  font?: number
  glyph?: number
  trailing?: React.ReactNode
  style?: React.CSSProperties
}> = ({ ident, title, status, prio = 0, assignee, h = 64, font = 23, glyph = 22, trailing, style }) => (
  <div
    style={{
      display: `flex`,
      alignItems: `center`,
      gap: font * 0.6,
      height: h,
      padding: `0 ${font * 0.9}px`,
      borderBottom: `1px solid ${C.strokeRow}`,
      color: C.text,
      fontFamily: UI,
      fontSize: font,
      ...style,
    }}
  >
    <StatusGlyph kind={status} size={glyph} />
    <Ident text={ident} size={font * 0.8} />
    <span
      style={{
        flex: 1,
        minWidth: 0,
        overflow: `hidden`,
        whiteSpace: `nowrap`,
        textOverflow: `ellipsis`,
        fontWeight: 500,
        letterSpacing: `-0.01em`,
      }}
    >
      {title}
    </span>
    {trailing}
    <PrioBars level={prio} size={font * 0.7} />
    {assignee ? (
      <Avatar initials={assignee.initials} hue={assignee.hue} size={font * 1.15} />
    ) : (
      <span
        style={{
          width: font * 1.15,
          height: font * 1.15,
          borderRadius: `50%`,
          border: `1.5px dashed ${C.strokeStrong}`,
          flexShrink: 0,
        }}
      />
    )}
  </div>
)

export const GroupBand: React.FC<{
  label: string
  count: number
  status: StatusKind
  h?: number
  font?: number
  style?: React.CSSProperties
}> = ({ label, count, status, h = 44, font = 17, style }) => (
  <div
    style={{
      display: `flex`,
      alignItems: `center`,
      gap: font * 0.55,
      height: h,
      padding: `0 ${font * 1.2}px`,
      background: `${STATUS_COLOR[status]}1a`,
      borderBottom: `1px solid ${C.strokeSection}`,
      fontFamily: UI,
      fontSize: font,
      fontWeight: 600,
      color: C.text,
      ...style,
    }}
  >
    <StatusGlyph kind={status} size={font * 1.05} />
    {label}
    <span style={{ color: C.dim, fontWeight: 500 }}>{count}</span>
  </div>
)

// ── Live dot + ripple ───────────────────────────────────────────────────────
export const LiveDot: React.FC<{ f: number; size?: number; color?: string }> = ({
  f,
  size = 10,
  color = C.green,
}) => {
  const p = (f % 36) / 36
  return (
    <span
      style={{
        position: `relative`,
        display: `inline-block`,
        width: size,
        height: size,
        flexShrink: 0,
      }}
    >
      <span style={{ position: `absolute`, inset: 0, borderRadius: `50%`, background: color }} />
      <span
        style={{
          position: `absolute`,
          inset: -p * size * 0.9,
          borderRadius: `50%`,
          border: `1.5px solid ${color}`,
          opacity: (1 - p) * 0.8,
        }}
      />
    </span>
  )
}

// An expanding ring from (x, y): radius 0→size over `dur` frames from `at`.
export const Ripple: React.FC<{
  f: number
  at: number
  x: number
  y: number
  size: number
  color: string
  dur?: number
  width?: number
}> = ({ f, at, x, y, size, color, dur = 22, width = 2 }) => {
  if (f < at || f > at + dur) return null
  const t = seg(f, at, at + dur)
  const r = size * t
  return (
    <div
      style={{
        position: `absolute`,
        left: x - r,
        top: y - r,
        width: r * 2,
        height: r * 2,
        borderRadius: `50%`,
        border: `${width}px solid ${color}`,
        opacity: (1 - t) * 0.9,
        pointerEvents: `none`,
      }}
    />
  )
}

// ── Kinetic type ────────────────────────────────────────────────────────────
// Per-glyph stagger: each character rises, sharpens and lands `step` frames
// after the previous.
export const Kinetic: React.FC<{
  text: string
  f: number
  at: number
  step?: number
  dur?: number
  rise?: number
  blur?: number
  style?: React.CSSProperties
}> = ({ text, f, at, step = 2, dur = 16, rise = 40, blur = 12, style }) => (
  <span style={{ display: `inline-flex`, whiteSpace: `pre`, ...style }}>
    {[...text].map((ch, i) => {
      const t = seg(f, at + i * step, at + i * step + dur)
      return (
        <span
          key={`${i}-${ch}`}
          style={{
            display: `inline-block`,
            opacity: t,
            translate: `0px ${rise * (1 - t)}px`,
            filter: t < 0.985 ? `blur(${blur * (1 - t)}px)` : undefined,
          }}
        >
          {ch}
        </span>
      )
    })}
  </span>
)

// ── Act caption ─────────────────────────────────────────────────────────────
// Bottom-left: a mono index line, a violet tick, the display line.
export const Caption: React.FC<{
  f: number
  at: number
  out: number
  index: string
  line: string
}> = ({ f, at, out, index, line }) => {
  if (f < at || f > out + 12) return null
  const ex = seg(f, out, out + 10, IN)
  const tick = seg(f, at + 3, at + 17)
  return (
    <div
      style={{
        position: `absolute`,
        left: 120,
        top: 850,
        opacity: 1 - ex,
        translate: `0px ${-16 * ex}px`,
        filter: ex > 0.02 ? `blur(${8 * ex}px)` : undefined,
      }}
    >
      <div
        style={{
          fontFamily: MONO,
          fontSize: 18,
          fontWeight: 500,
          letterSpacing: `0.22em`,
          textTransform: `uppercase`,
          color: C.muted,
          ...enter(f, at, 12, { rise: 14, blur: 6 }),
        }}
      >
        {index}
      </div>
      <div
        style={{
          width: 72 * tick,
          height: 3,
          borderRadius: 2,
          background: VIOLET,
          margin: `14px 0 16px`,
        }}
      />
      <div
        style={{
          fontFamily: DISPLAY,
          fontSize: 48,
          fontWeight: 600,
          letterSpacing: `-0.025em`,
          color: C.text,
          lineHeight: 1.1,
          ...enter(f, at + 4, 16, { rise: 22, blur: 10 }),
        }}
      >
        {line}
      </div>
    </div>
  )
}

// ── Cursor ──────────────────────────────────────────────────────────────────
export const Cursor: React.FC<{ x: number; y: number; press?: number }> = ({
  x,
  y,
  press = 0,
}) => (
  <div style={{ position: `absolute`, left: x, top: y, zIndex: 90 }}>
    {press > 0 && press < 1 ? (
      <div
        style={{
          position: `absolute`,
          left: -6 - press * 16,
          top: -6 - press * 16,
          width: 12 + press * 32,
          height: 12 + press * 32,
          borderRadius: `50%`,
          border: `2px solid rgba(255,255,255,${0.5 * (1 - press)})`,
        }}
      />
    ) : null}
    <svg
      width={26}
      height={26}
      viewBox="0 0 24 24"
      style={{
        scale: String(press > 0 && press < 0.5 ? 0.88 : 1),
        filter: `drop-shadow(0 2px 6px rgba(0,0,0,0.7))`,
      }}
    >
      <path
        d="M5 3 L19 12.5 L12.6 13.8 L15.5 20 L13 21 L10.2 14.8 L5 19 Z"
        fill="#fafafa"
        stroke="#111"
        strokeWidth={1.2}
        strokeLinejoin="round"
      />
    </svg>
  </div>
)
