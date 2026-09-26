// showreel/timeline.ts — six acts over 15 s @ 30 fps (EXP-1100). Adjacent
// acts overlap by 6 frames: the outgoing act plays its exit inside the
// incoming act's rise, so the canvas never rests bare between them. Acts are
// authored at LOCAL frame 0; Showreel.tsx hands each its local frame.

export const FPS = 30
export const DURATION_IN_FRAMES = 450

export type Act = { id: string; from: number; to: number }

export const ACTS = {
  ignition: { id: `ignition`, from: 0, to: 80 },
  board: { id: `board`, from: 66, to: 162 },
  agents: { id: `agents`, from: 154, to: 250 },
  everywhere: { id: `everywhere`, from: 240, to: 338 },
  ship: { id: `ship`, from: 328, to: 416 },
  outro: { id: `outro`, from: 406, to: DURATION_IN_FRAMES },
} as const satisfies Record<string, Act>

// The brand lockup collapses into its corner chip here, and returns for the
// outro (Showreel.tsx `Lockup`).
export const LOCKUP_TO_CHIP = 60
export const LOCKUP_RETURN = 412
