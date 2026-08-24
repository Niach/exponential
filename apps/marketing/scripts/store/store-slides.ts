/* The store-screenshot PLAN (EXP-566): every string of copy, the slide order,
   and the canvas table. Nothing here renders — `store-styles.tsx` turns a slide
   plus its raw capture into pixels, `render-store-screenshots.tsx` drives it.

   `shot` is the capture key the UI tests emit (`01_board` → `board`), parsed by
   `shotKey()` in raw-store.ts. A slide with `shot: null` needs no capture. */

import type { Form, Platform } from "./raw-store"

/** Which style set ships. `--proposals` renders all three side by side. */
export const PRODUCTION_SET = `zinc`

export const STYLE_SET_IDS = [`aurora`, `zinc`, `colorblock`] as const
export type StyleSetId = (typeof STYLE_SET_IDS)[number]

/* ── Devices ─────────────────────────────────────────────────────────────────
   iOS: the two required App Store slots. ASC rejects off-size 6.9"/13" images,
   so the canvas IS the raw size and any drift is a hard error.

   Android: Play requires the longer side to be at most 2× the shorter one, and
   the phone emulator captures 1080×2400 (2.22:1). So the Android canvas is a
   FIXED 1080×1920 and the raw is letterboxed inside the bezel at its own aspect
   ratio rather than being stretched. */

export type DeviceSpec = {
  form: Form
  platform: Platform
  /** Output canvas in real pixels. */
  width: number
  height: number
  /** Expected raw capture size. */
  rawWidth: number
  rawHeight: number
  /** iOS: the raw must equal the canvas exactly. */
  strictRawSize: boolean
  /** fastlane snapshot prefixes uploads with the simulator name. */
  filePrefix: string
  label: string
}

export const DEVICES: Record<Form, DeviceSpec> = {
  "ios-phone": {
    form: `ios-phone`,
    platform: `ios`,
    width: 1320,
    height: 2868,
    rawWidth: 1320,
    rawHeight: 2868,
    strictRawSize: true,
    filePrefix: `iPhone 17 Pro Max`,
    label: `iPhone 6.9"`,
  },
  "ios-tablet": {
    form: `ios-tablet`,
    platform: `ios`,
    width: 2064,
    height: 2752,
    rawWidth: 2064,
    rawHeight: 2752,
    strictRawSize: true,
    filePrefix: `iPad Pro 13-inch (M5)`,
    label: `iPad 13"`,
  },
  "android-phone": {
    form: `android-phone`,
    platform: `android`,
    width: 1080,
    height: 1920,
    rawWidth: 1080,
    rawHeight: 2400,
    strictRawSize: false,
    filePrefix: ``,
    label: `Android phone`,
  },
}

/** Play's feature graphic — its own canvas, no capture behind it. */
export const FEATURE_GRAPHIC = { width: 1024, height: 500 }

/** Play listing rule: longer side ≤ 2× shorter side. */
export function playRatioOk(width: number, height: number): boolean {
  return Math.max(width, height) <= 2 * Math.min(width, height)
}

/* ── Slides ──────────────────────────────────────────────────────────────── */

export type Slide = {
  id: string
  /** 1-based, drives the `NN_` filename prefix. */
  index: number
  /** Capture key behind the bezel; null = type-only slide. */
  shot: string | null
  /** Hero only: a capture used as decoration, entering at an angle. */
  decorShot?: string
  eyebrow?: string
  /** One array entry per rendered line. */
  headline: string[]
  sub?: string
  forms: Form[]
  /** Play's 1024×500 feature graphic, not a phone screenshot. */
  featureGraphic?: boolean
}

const PHONES: Form[] = [`ios-phone`, `ios-tablet`, `android-phone`]

/* Copy is positioning, not a feature walkthrough — keep every line short enough
   to survive the 1080-wide Android canvas. The hero headline and sub are the
   store listing's own opening lines (apps/ios/fastlane/metadata/en-US/
   description.txt); keep them in step with it. */
export const SLIDES: Slide[] = [
  {
    id: `hero`,
    index: 1,
    shot: null,
    decorShot: `board`,
    eyebrow: `Exponential`,
    headline: [`Vibecode`, `together.`],
    sub: `Issues, customer support and coding agents in one workspace, in sync on every device.`,
    forms: PHONES,
  },
  {
    id: `board`,
    index: 2,
    shot: `board`,
    eyebrow: `Boards`,
    headline: [`The next-gen`, `dev platform`],
    sub: `Issues, boards and realtime sync`,
    forms: PHONES,
  },
  {
    id: `start-coding`,
    index: 3,
    shot: `start-coding`,
    eyebrow: `Remote start`,
    headline: [`Start coding`, `from your phone`],
    sub: `Pick the agent, branch and machine`,
    forms: PHONES,
  },
  {
    id: `steering`,
    index: 4,
    shot: `steering`,
    eyebrow: `Live steering`,
    headline: [`Local agents.`, `Your machines.`],
    sub: `Watch a run live — and steer it from anywhere`,
    forms: PHONES,
  },
  {
    id: `review`,
    index: 5,
    shot: `review`,
    eyebrow: `Reviews`,
    headline: [`Review`, `and merge`],
    sub: `Every open PR, one tap to squash-merge`,
    forms: PHONES,
  },
  {
    id: `actions`,
    index: 6,
    shot: `actions`,
    eyebrow: `Actions`,
    headline: [`AI actions.`, `Automate anything.`],
    sub: `Saved agent playbooks your team can run`,
    forms: PHONES,
  },
  {
    id: `inbox`,
    index: 7,
    shot: `inbox`,
    eyebrow: `Inbox`,
    headline: [`A calm`, `inbox`],
    sub: `Assignments, mentions and merged PRs`,
    forms: PHONES,
  },
  {
    id: `support`,
    index: 8,
    shot: `support`,
    eyebrow: `Helpdesk`,
    headline: [`Support`, `built in`],
    sub: `Customer tickets land next to the code`,
    forms: PHONES,
  },
  {
    // iOS ships 9 slots; Play caps the phone set at 8, so this one is iOS-only.
    id: `issue-detail`,
    index: 9,
    shot: `issue-detail`,
    eyebrow: `Issues`,
    headline: [`Everything`, `on the issue`],
    sub: `Markdown, checklists, mentions — live on every device`,
    forms: [`ios-phone`, `ios-tablet`],
  },
  {
    id: `feature-graphic`,
    index: 0,
    shot: null,
    headline: [`Vibecode together.`],
    sub: `The next-gen dev platform`,
    forms: [`android-phone`],
    featureGraphic: true,
  },
]

export function slidesFor(form: Form): Slide[] {
  return SLIDES.filter((s) => s.forms.includes(form))
}

export function slideById(id: string): Slide | undefined {
  return SLIDES.find((s) => s.id === id)
}

/** `02_board.png`, zero-padded — the order fastlane/supply upload in. */
export function slideFileName(slide: Slide): string {
  return `${String(slide.index).padStart(2, `0`)}_${slide.id}.png`
}
