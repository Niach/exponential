/* Motion's trees on the site: the guide's spinner and check, the
   interactive like button and level bar (state machines), the eight
   gallery tiles (trees.ts) and the Lottie → dialect → compile showcase.
   Every Motion paints client-side (WASM engine + Canvas2D), so each tree
   here mounts as a `client:visible` EuiSurface. */
import type { NestedNode } from "@exponential_app/eui"
import report from "@exponential_app/eui/eval/motion/report.json"
import lottieReply from "@exponential_app/eui/eval/motion/fixtures/lottie/check-draw.json"
import dialectReply from "@exponential_app/eui/eval/motion/fixtures/dialect/check-draw.json"
import { motionWasmSync } from "@exponential_app/eui/motion/server"

type Props = Record<string, unknown>

/** The ```json block of a recorded model reply. */
export function replyJson(text: string): Props {
  const m = /```json([\s\S]*?)```/.exec(text)
  return JSON.parse(m ? m[1]! : text) as Props
}

export const SPINNER: Props = {
  size: [48, 48],
  duration: 1000,
  label: `Loading`,
  layers: [
    {
      type: `shape`,
      transform: { position: [24, 24], rotation: { keys: [{ t: 0, v: 0 }, { t: 1000, v: 360 }] } },
      shapes: [{ type: `ellipse`, size: [36, 36] }, { type: `trim`, end: 70 }, { type: `stroke`, color: `$color.primary`, width: 4, cap: `round` }],
    },
  ],
}

export const CHECK: Props = {
  size: [64, 64],
  duration: 1200,
  loop: false,
  label: `Done`,
  layers: [
    {
      type: `shape`,
      shapes: [
        { type: `path`, d: `M20 33 L28 41 L44 24` },
        { type: `trim`, end: { keys: [{ t: 0, v: 0 }, { t: 450, v: 0, ease: `ease-out` }, { t: 800, v: 100 }] } },
        { type: `stroke`, color: `$color.success`, width: 5, cap: `round`, join: `round` },
      ],
    },
    {
      type: `shape`,
      shapes: [
        { type: `ellipse`, position: [32, 32], size: [52, 52] },
        { type: `trim`, end: { keys: [{ t: 0, v: 0, ease: `ease-in-out` }, { t: 500, v: 100 }] } },
        { type: `stroke`, color: `$color.success`, width: 4, cap: `round` },
      ],
    },
  ],
}

/** The spinner and the check side by side (the guide's first example). */
export const FIRST: NestedNode = {
  id: `first`,
  component: `Stack`,
  props: { direction: `horizontal`, gap: `xl`, align: `center`, justify: `center` },
  children: [
    { id: `spinner`, component: `Motion`, props: SPINNER },
    { id: `check`, component: `Motion`, props: CHECK },
  ],
}

const HEART = `M24 39 C24 39 9 30 9 19 C9 14 13 10 18 10 C21 10 23 12 24 14 C25 12 27 10 30 10 C35 10 39 14 39 19 C39 30 24 39 24 39 Z`
const toggleOn = (to: string, liked: boolean) => ({ type: `Transition`, toState: to, guards: [{ type: `Boolean`, inputName: `liked`, conditionType: `Equal`, compareTo: liked }] })

/** A like button: a Click interaction toggles `liked`; `off` / `on` play their markers. */
export const LIKE: Props = {
  size: [48, 48],
  duration: 1000,
  label: `Like`,
  markers: [
    { name: `off`, time: 0, duration: 300 },
    { name: `on`, time: 300, duration: 700 },
  ],
  layers: [
    {
      type: `shape`,
      transform: {
        anchor: [24, 24],
        position: [24, 24],
        scale: { keys: [{ t: 0, v: [60, 60], ease: `hold` }, { t: 300, v: [60, 60], ease: `ease-out` }, { t: 520, v: [125, 125], ease: `ease-in-out` }, { t: 760, v: [100, 100] }] },
        opacity: { keys: [{ t: 0, v: 0, ease: `hold` }, { t: 300, v: 100 }] },
      },
      shapes: [{ type: `path`, d: HEART }, { type: `fill`, color: `$color.destructive` }],
    },
    {
      type: `shape`,
      transform: { anchor: [24, 24], position: [24, 24], scale: { keys: [{ t: 0, v: [115, 115], ease: `ease-out` }, { t: 300, v: [100, 100] }] } },
      shapes: [{ type: `path`, d: HEART }, { type: `stroke`, color: `$color.foreground`, width: 3, join: `round` }],
    },
  ],
  stateMachine: {
    initial: `off`,
    inputs: [{ type: `Boolean`, name: `liked`, value: false }],
    interactions: [{ type: `Click`, actions: [{ type: `Toggle`, inputName: `liked` }] }],
    states: [
      { name: `off`, type: `PlaybackState`, marker: `off`, loop: false, transitions: [toggleOn(`on`, true)] },
      { name: `on`, type: `PlaybackState`, marker: `on`, loop: false, transitions: [toggleOn(`off`, false)] },
    ],
  },
}

const level = (to: string, op: string, v: number) => ({ type: `Transition`, toState: to, guards: [{ type: `Numeric`, inputName: `level`, conditionType: op, compareTo: v }] })
const BAR = `M12 24 L164 24`
const TRIM = { type: `trim`, end: { keys: [{ t: 0, v: 0, ease: `ease-out` }, { t: 400, v: 33, ease: `ease-out` }, { t: 800, v: 70, ease: `ease-out` }, { t: 1100, v: 100 }] } }

/** A level bar: its `low` / `high` / `done` states follow the bound `inputs`. */
export const LEVEL: Props = {
  size: [200, 48],
  duration: 1300,
  label: `Upload level`,
  inputs: { path: `/meter` },
  markers: [
    { name: `low`, time: 0, duration: 400 },
    { name: `high`, time: 400, duration: 400 },
    { name: `done`, time: 800, duration: 500 },
  ],
  layers: [
    {
      type: `shape`,
      transform: { opacity: { keys: [{ t: 0, v: 0, ease: `hold` }, { t: 1000, v: 0, ease: `ease-out` }, { t: 1250, v: 100 }] } },
      shapes: [
        { type: `path`, d: `M179 24 L183 28 L190 20` },
        { type: `stroke`, color: `$color.background`, width: 3, cap: `round`, join: `round` },
        { type: `ellipse`, position: [184, 24], size: [22, 22] },
        { type: `fill`, color: `$color.success` },
      ],
    },
    { type: `shape`, shapes: [{ type: `path`, d: BAR }, TRIM, { type: `stroke`, color: `$color.primary`, width: 8, cap: `round` }] },
    { type: `shape`, shapes: [{ type: `path`, d: BAR }, { type: `stroke`, color: `$color.muted`, width: 8, cap: `round` }] },
  ],
  stateMachine: {
    initial: `low`,
    inputs: [{ type: `Numeric`, name: `level`, value: 0 }],
    states: [
      { name: `low`, type: `PlaybackState`, marker: `low`, loop: false, transitions: [level(`high`, `GreaterThanOrEqual`, 50)] },
      { name: `high`, type: `PlaybackState`, marker: `high`, loop: false, transitions: [level(`done`, `Equal`, 100), level(`low`, `LessThan`, 50)] },
      { name: `done`, type: `PlaybackState`, marker: `done`, loop: false, final: true, transitions: [level(`low`, `LessThan`, 50), level(`high`, `LessThan`, 100)] },
    ],
  },
}

/** The like button and the level bar on a Slider. */
export const INTERACTIVE: NestedNode = {
  id: `interactive`,
  component: `Stack`,
  props: { direction: `horizontal`, gap: `xl`, align: `center`, justify: `center`, wrap: true },
  children: [
    { id: `like`, component: `Motion`, props: LIKE, on: { press: { event: { name: `like` } } } },
    {
      id: `meter`,
      component: `Stack`,
      props: { direction: `vertical`, gap: `xs`, align: `stretch` },
      style: { width: `200px` },
      children: [
        { id: `level`, component: `Motion`, props: LEVEL, on: { state: { event: { name: `level-state` } }, complete: { event: { name: `uploaded` } } } },
        { id: `level-slider`, component: `Slider`, props: { label: `Level`, min: 0, max: 100, step: 10, value: { path: `/meter/level` } } },
      ],
    },
  ],
} as NestedNode
export const INTERACTIVE_DATA = { meter: { level: 30 } }

/* The showcase: one prompt, answered once as raw Lottie and once in the
   dialect (the eval's recorded first tries), and the dialect compiled to
   Lottie at build time by the same WASM engine the players run. */
const DIALECT = replyJson(dialectReply.text)
const LOTTIE = replyJson(lottieReply.text)
const compiled = JSON.parse(motionWasmSync().compile(JSON.stringify(DIALECT))) as { lottie?: Props; errors?: unknown[] }
if (!compiled.lottie) throw new Error(`showcase: the dialect did not compile: ${JSON.stringify(compiled.errors)}`)
const COMPILED = compiled.lottie

const json = (v: unknown) => JSON.stringify(v, null, 2)
const tokens = (n: number) => n.toLocaleString(`en-US`)

export const SHOWCASE = {
  prompt: dialectReply.prompt,
  steps: [
    { id: `lottie`, title: `Lottie`, meta: `${tokens(lottieReply.usage.outputTokens)} tokens`, code: json(LOTTIE), props: { lottie: LOTTIE, label: `Lottie` } as Props },
    { id: `dialect`, title: `Dialect`, meta: `${tokens(dialectReply.usage.outputTokens)} tokens`, code: json(DIALECT), props: { ...DIALECT, label: `Dialect` } },
    { id: `compiled`, title: `Compiled`, meta: `${tokens(JSON.stringify(COMPILED).length)} bytes`, code: json(COMPILED), props: { lottie: COMPILED, label: `Compiled` } as Props },
  ],
}

/** One live player per showcase step. */
export const SHOWCASE_TREE: NestedNode = {
  id: `showcase`,
  component: `Grid`,
  props: { columns: 3, gap: `md` },
  children: SHOWCASE.steps.map((s) => ({
    id: `s-${s.id}`,
    component: `Stack`,
    props: { gap: `xs`, align: `center` },
    children: [
      { id: `m-${s.id}`, component: `Motion`, props: s.props, style: { width: `96px`, height: `96px` } },
      { id: `t-${s.id}`, component: `Text`, props: { text: s.title, variant: `muted` } },
    ],
  })),
}

/** The eval's totals: first-try validity and mean output tokens per format. */
export const EVAL = {
  dialect: report.totals.dialect,
  lottie: report.totals.lottie,
  ratio: report.totals.lottie.meanOutputTokens / report.totals.dialect.meanOutputTokens,
}
