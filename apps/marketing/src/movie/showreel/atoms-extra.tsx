// showreel/atoms-extra.tsx — atoms the variant reels add (EXP-1100 r2):
// decode-in text (characters resolve left to right out of noise), the
// slot-machine digit roll, a timecode, and the shared brand end card.

import React from "react"
import { random } from "remotion"
import { ExpLogo } from "../ships/rig"
import { C, DISPLAY, MONO } from "./theme"
import { Easing } from "remotion"
import { SETTLE, enter, pop, seg } from "./motion"
import { Kinetic } from "./atoms"

const NOISE = `ABCDEFGHJKLMNPQRSTUVWXYZ0123456789#/<>_-=+*`

// Characters resolve left to right over `dur`; the three ahead of the
// resolve point flicker through noise, the rest are not there yet.
export const Decode: React.FC<{
  text: string
  f: number
  at: number
  dur?: number
  style?: React.CSSProperties
}> = ({ text, f, at, dur = 22, style }) => {
  if (f < at) return null
  const t = seg(f, at, at + dur, Easing.linear)
  const resolved = Math.floor(t * (text.length + 3))
  const chars = [...text].map((ch, i) => {
    if (ch === ` `) return ` `
    if (i < resolved - 3 || t >= 1) return ch
    if (i >= resolved) return ``
    const r = random(`${i}-${Math.floor(f / 2)}-${text.length}`)
    return NOISE[Math.floor(r * NOISE.length)]
  })
  return <span style={{ whiteSpace: `pre`, ...style }}>{chars.join(``)}</span>
}

// One digit column that rolls from `from` to `to` (continuous, eased).
export const DigitRoll: React.FC<{
  f: number
  at: number
  dur: number
  from: number
  to: number
  size: number
  style?: React.CSSProperties
}> = ({ f, at, dur, from, to, size, style }) => {
  const v = from + (to - from) * seg(f, at, at + dur)
  return (
    <span
      style={{
        display: `inline-block`,
        height: size,
        overflow: `hidden`,
        lineHeight: 1,
        verticalAlign: `top`,
        ...style,
      }}
    >
      <span style={{ display: `block`, translate: `0px ${-v * size}px` }}>
        {Array.from({ length: 10 }, (_, d) => (
          <span key={d} style={{ display: `block`, height: size, lineHeight: 1 }}>
            {d}
          </span>
        ))}
      </span>
    </span>
  )
}

export const timecode = (f: number, fps = 30): string => {
  const s = Math.floor(f / fps)
  const ff = f % fps
  const pad = (n: number) => String(n).padStart(2, `0`)
  return `00:00:${pad(s)}:${pad(ff)}`
}

// The brand end card: mark draws, wordmark lands letter by letter, the site
// line rises. `mono` swaps the display face for the mono one (blueprint).
export const BrandCard: React.FC<{
  f: number
  at: number
  mono?: boolean
  sub?: string
}> = ({ f, at, mono = false, sub = `exponential.at` }) => {
  if (f < at) return null
  const l = f - at
  const logoS = 0.5 + 0.5 * pop(l, 0, SETTLE)
  return (
    <div
      style={{
        position: `absolute`,
        inset: 0,
        display: `flex`,
        flexDirection: `column`,
        alignItems: `center`,
        justifyContent: `center`,
        gap: 40,
      }}
    >
      <div style={{ display: `flex`, alignItems: `center`, gap: 32 }}>
        <div style={{ width: 150, height: 150, scale: String(logoS), filter: `drop-shadow(0 0 40px rgba(255,255,255,0.18))` }}>
          <ExpLogo size={150} drawT={seg(l, 2, 30)} discO={seg(l, 0, 8)} />
        </div>
        <div
          style={{
            fontFamily: mono ? MONO : DISPLAY,
            fontSize: mono ? 110 : 140,
            fontWeight: 600,
            letterSpacing: mono ? `0.02em` : `-0.035em`,
            color: C.text,
            lineHeight: 1,
            whiteSpace: `nowrap`,
          }}
        >
          {mono ? (
            <Decode text="EXPONENTIAL" f={l} at={8} dur={28} />
          ) : (
            <Kinetic text="Exponential" f={l} at={8} step={2} dur={18} rise={56} blur={16} />
          )}
        </div>
      </div>
      <div
        style={{
          fontFamily: MONO,
          fontSize: 30,
          fontWeight: 500,
          letterSpacing: `0.04em`,
          color: mono ? C.green : C.text,
          ...enter(l, 30, 14, { rise: 18, blur: 8 }),
        }}
      >
        {sub}
      </div>
    </div>
  )
}
