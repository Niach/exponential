// showreel/motion.ts — the reel's motion vocabulary (EXP-1100): one expo-out
// curve for every eased move, three named springs for pops, and the
// enter/exit envelopes (opacity + rise + defocus) every element shares, so
// the whole film moves with ONE hand.

import { Easing, interpolate, spring } from "remotion"

export const FPS = 30

// Expo-out (the rig's EASE): fast attack, long settle.
export const OUT = Easing.bezier(0.16, 1, 0.3, 1)
// Accelerating exit.
export const IN = Easing.bezier(0.7, 0, 0.84, 0)
export const IN_OUT = Easing.bezier(0.65, 0, 0.35, 1)

export const CL = {
  extrapolateLeft: "clamp",
  extrapolateRight: "clamp",
} as const

// 0→1 across [from, to], clamped, eased.
export const seg = (
  f: number,
  from: number,
  to: number,
  easing: (t: number) => number = OUT
): number => interpolate(f, [from, to], [0, 1], { ...CL, easing })

export const lerp = (a: number, b: number, t: number): number =>
  a + (b - a) * t

export const mix = (
  f: number,
  from: number,
  to: number,
  a: number,
  b: number,
  easing: (t: number) => number = OUT
): number => lerp(a, b, seg(f, from, to, easing))

export type SpringCfg = { damping: number; stiffness: number; mass?: number }
export const POP: SpringCfg = { damping: 12, stiffness: 200, mass: 0.8 }
export const SNAP: SpringCfg = { damping: 14, stiffness: 320, mass: 0.6 }
export const SETTLE: SpringCfg = { damping: 18, stiffness: 150 }
export const SOFT: SpringCfg = { damping: 22, stiffness: 90 }

// A spring that starts at `at` (0 before it).
export const pop = (f: number, at: number, config: SpringCfg = POP): number =>
  f < at ? 0 : spring({ frame: f - at, fps: FPS, config })

// Entrance envelope: opacity + rise + defocus (defocus dropped once sharp so
// resting text stays crisp).
export const enter = (
  f: number,
  at: number,
  dur = 14,
  opts: { rise?: number; x?: number; blur?: number } = {}
): { opacity: number; translate: string; filter: string | undefined } => {
  const { rise = 24, x = 0, blur = 10 } = opts
  const t = seg(f, at, at + dur)
  return {
    opacity: t,
    translate: `${x * (1 - t)}px ${rise * (1 - t)}px`,
    filter: blur > 0 && t < 0.985 ? `blur(${blur * (1 - t)}px)` : undefined,
  }
}

// Exit envelope (accelerating): opacity + lift + defocus.
export const exit = (
  f: number,
  at: number,
  dur = 10,
  opts: { rise?: number; blur?: number } = {}
): { opacity: number; translate: string; filter: string | undefined } => {
  const { rise = -16, blur = 8 } = opts
  const t = seg(f, at, at + dur, IN)
  return {
    opacity: 1 - t,
    translate: `0px ${rise * t}px`,
    filter: blur > 0 && t > 0.015 ? `blur(${blur * t}px)` : undefined,
  }
}

// A staggered start frame.
export const stagger = (at: number, i: number, step: number): number =>
  at + i * step

// Chars typed by frame `f` from `at` at `cpf` chars per frame.
export const typed = (text: string, f: number, at: number, cpf = 1): string =>
  f < at ? `` : text.slice(0, Math.max(0, Math.floor((f - at) * cpf)))

// A rolling integer.
export const roll = (
  f: number,
  from: number,
  to: number,
  a: number,
  b: number
): number => Math.round(mix(f, from, to, a, b))
