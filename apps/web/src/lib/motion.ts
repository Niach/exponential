// EXP-523: the shared motion durations, in milliseconds, for the few places
// that need them in JS rather than CSS — an exit animation's unmount timer,
// for example, where the DOM node has to outlive the state change.
//
// Hand-authored and parity-tested against packages/design-tokens/tokens.json
// by `design-tokens.test.ts`, exactly like the `--motion-*` vars in styles.css.
// Anything that can be expressed in CSS should use the `duration-fast` /
// `duration-standard` / `duration-slow` utilities instead of importing these.

export const MOTION_DURATION_MS = {
  fast: 120,
  standard: 180,
  slow: 280,
} as const

export type MotionSpeed = keyof typeof MOTION_DURATION_MS
