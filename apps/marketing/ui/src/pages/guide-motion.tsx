/* EXP-1604: the Motion guide (eUI's vector animations): what it is, the
   dialect a person or a model writes (packages/exponential-ui/eval/motion/
   guide.md, the same text models get), the three sources, theming, the
   playback rules every renderer keeps, accessibility. The live example is
   the tree printed beside it. */
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
    id: `a11y`,
    label: `Accessibility`,
    body: (
      <p>
        With a <code>label</code> a Motion is an image named by it; without one it is decorative and hidden from
        assistive technology. It is never focusable.
      </p>
    ),
  },
]
