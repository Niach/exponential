/* EXP-1604: the Motion guide (eUI's vector animations): what it is, the
   dialect a person or a model writes (packages/exponential-ui/eval/motion/
   guide.md, the same text models get), the three sources, theming, the
   playback rules every renderer keeps, accessibility. The live example is
   the tree printed beside it. EXP-1605: Interactivity (state machines,
   inputs, events), with a live like button and a level bar on a Slider. */
import type { ReactNode } from "react"
import { DocsCode } from "@exp/site-shell"
import { DocsTable } from "../components/Content"
import { guideSource } from "../lib/guide-source"
import { useScheme } from "../lib/scheme"
import { LINKS } from "../lib/site"
import type { Nested } from "../sdk/a2ui"
import { MiniSurface } from "../sdk/MiniSurface"

type Section = { id: string; label: string; body: ReactNode }

const SPINNER = {
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

const CHECK = {
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

const TREE: Nested = {
  id: `motion-guide`,
  component: `Stack`,
  props: { direction: `horizontal`, gap: `xl`, align: `center`, justify: `center` },
  children: [
    { id: `spinner`, component: `Motion`, props: SPINNER },
    { id: `check`, component: `Motion`, props: CHECK },
  ],
} as Nested

const HEART = `M24 39 C24 39 9 30 9 19 C9 14 13 10 18 10 C21 10 23 12 24 14 C25 12 27 10 30 10 C35 10 39 14 39 19 C39 30 24 39 24 39 Z`
const toggleOn = (to: string, liked: boolean) => ({ type: `Transition`, toState: to, guards: [{ type: `Boolean`, inputName: `liked`, conditionType: `Equal`, compareTo: liked }] })

/** A like button: a Click interaction toggles `liked`; `off` / `on` play their markers. */
const LIKE = {
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
const LEVEL = {
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

const INTERACTIVE: Nested = {
  id: `motion-interactive`,
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
} as Nested
const INTERACTIVE_DATA = { meter: { level: 30 } }

function InteractiveExample() {
  const mode = useScheme()
  return <MiniSurface domId="motion-interactive" tree={INTERACTIVE} data={INTERACTIVE_DATA} mode={mode} className="sdk-mini-wide" />
}

function LiveExample() {
  const mode = useScheme()
  return <MiniSurface domId="motion-guide" tree={TREE} mode={mode} className="sdk-mini-wide" />
}

export const MOTION_SECTIONS: Section[] = [
  {
    id: `what`,
    label: `What Motion is`,
    body: (
      <>
        <p>
          <code>Motion</code> is a vector animation in a surface: a loader, a success state, an illustrated empty
          state, a small explainer. One engine (<code>eui-motion</code>, Rust) loads the animation and samples every
          frame into a flat display list; each renderer paints that list and nothing else, so React, Solid, the web
          component, SwiftUI, Compose and gpui show the same picture. The web renderers share one Canvas2D painter and
          load the engine as WebAssembly on the first Motion they paint (about 200 KB gzipped, never in your initial
          bundle; bundlers emit it as a hashed asset).
        </p>
        <LiveExample />
        <DocsCode language="json">{JSON.stringify({ component: `Motion`, props: SPINNER }, null, 2)}</DocsCode>
      </>
    ),
  },
  {
    id: `dialect`,
    label: `The dialect`,
    body: (
      <>
        <p>
          The props of a <code>Motion</code> node are the animation itself: <code>size</code>, <code>duration</code>{` `}
          (ms), <code>fps</code> and <code>layers</code>, readable JSON that compiles to Lottie. This is the whole
          guide a model gets in its prompt; with it, models write valid animations in about a third of the tokens raw
          Lottie takes (<a href={LINKS.source(`packages/exponential-ui/eval/motion`)}>the eval</a>).
        </p>
        <DocsCode language="markdown">{guideSource(`eui:eval/motion/guide.md`)}</DocsCode>
      </>
    ),
  },
  {
    id: `sources`,
    label: `Sources`,
    body: (
      <DocsTable
        head={[`Prop`, `What`]}
        rows={[
          [`layers (+ size, duration, fps, assets, markers)`, `The dialect above, inline in the surface.`],
          [`lottie`, `A Lottie JSON animation as is. Expressions are refused; 3D layers are not drawn.`],
          [`src`, `An attached .json or .lottie file (the first animation of a .lottie, its images inlined), fetched under the host's media policy.`],
        ]}
      />
    ),
  },
  {
    id: `theming`,
    label: `Theming`,
    body: (
      <p>
        A colour written as <code>$color.primary</code> (or any colour token) becomes a Lottie slot the renderer fills
        with the theme's colour for the current mode, and refills when the theme or mode changes; a <code>{`{light, dark}`}</code>{` `}
        pair picks its half by mode. A Lottie file is recoloured through <code>slots</code> (<code>{`{slotId: value}`}</code>).
      </p>
    ),
  },
  {
    id: `playback`,
    label: `Playback`,
    body: (
      <>
        <p>One clock, the engine's, on every platform:</p>
        <DocsTable
          head={[`Prop or rule`, `Effect`]}
          rows={[
            [`autoplay, loop, plays, direction, speed`, `Plays when visible; repeats, plays n times, reverses or alternates; scales time.`],
            [`segment, marker`, `Plays only [from, to] ms, or a named marker's segment.`],
            [`progress (bindable)`, `Scrubs to 0–1 instead of playing: bind it to drive the animation from data.`],
            [`play (bindable)`, `false freezes the clock where it is.`],
            [`poster`, `The still frame (ms) shown before playing, off-screen and under reduced motion.`],
            [`Frame rate`, `At most 60 fps (the display's rate when lower).`],
            [`Visibility`, `Paused off-screen and while the window is hidden; at most 8 Motions animate per surface (tree order), the rest show their poster.`],
            [`Reduced motion`, `The poster, never ticking.`],
          ]}
        />
        <p>
          Unstyled, the box is the composition's px size, shrinking to the available width with its aspect kept; a style
          width or height wins. <code>fit</code> is contain (default), cover or fill.
        </p>
      </>
    ),
  },
  {
    id: `interactivity`,
    label: `Interactivity`,
    body: (
      <>
        <p>
          A <code>stateMachine</code> (dotLottie 2 JSON; a .lottie <code>src</code> brings its own first machine) turns
          a Motion into a control. Its states pick what plays (a segment or a marker, loop, speed, autoplay,{` `}
          <code>final</code>); transitions guard on inputs; interactions (Click, PointerDown/Up, PointerEnter/Exit,
          OnComplete) run actions on them. The engine interprets the machine, the same code on every platform; a
          renderer only reports what happened and restarts the clock when the state changes. Click the heart; drag the
          slider past 50 and to 100.
        </p>
        <InteractiveExample />
        <DocsTable
          head={[`Prop or event`, `Effect`]}
          rows={[
            [`stateMachine`, `The machine; while it runs, its current state's playback replaces the playback props.`],
            [`inputs (bindable)`, `{name: value} written into the machine on every change: bind it so a data model drives the states.`],
            [`Interactions`, `A press (pointer, or Enter / Space when focusable) is Click; pointer down / up; hover enters and leaves; a finished play is OnComplete.`],
            [`on.press`, `A press. With it the Motion is a button: focusable, named by its label.`],
            [`on.hover`, `The pointer entered.`],
            [`on.visible`, `The first time half of it is on screen.`],
            [`on.complete`, `A finite play finished, or the machine reached a final state.`],
            [`on.state`, `Each state the machine enters, with context {state}.`],
          ]}
        />
        <DocsCode language="json">{JSON.stringify({ component: `Motion`, props: { stateMachine: LIKE.stateMachine, markers: LIKE.markers }, on: { press: { event: { name: `like` } } } }, null, 2)}</DocsCode>
      </>
    ),
  },
  {
    id: `a11y`,
    label: `Accessibility`,
    body: (
      <p>
        With a <code>label</code> a Motion is an image named by it; without one it is decorative and hidden from
        assistive technology. With <code>on.press</code> it is a button named by its label: focusable, pressed with
        Enter or Space.
      </p>
    ),
  },
]
