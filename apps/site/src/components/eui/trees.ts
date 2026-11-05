/* The trees the spike renders. Static ones are what an agent would send;
   the Motion gallery is the eval's recorded model replies (the same eight
   the current Motion guide shows). */
import type { NestedNode } from "@exponential_app/eui"
import pulseDot from "@exponential_app/eui/eval/motion/fixtures/dialect/pulse-dot.json"
import progressRing from "@exponential_app/eui/eval/motion/fixtures/dialect/progress-ring.json"
import typingDots from "@exponential_app/eui/eval/motion/fixtures/dialect/typing-dots.json"
import heartbeat from "@exponential_app/eui/eval/motion/fixtures/dialect/heartbeat.json"
import equalizer from "@exponential_app/eui/eval/motion/fixtures/dialect/equalizer.json"
import errorX from "@exponential_app/eui/eval/motion/fixtures/dialect/error-x.json"
import sun from "@exponential_app/eui/eval/motion/fixtures/dialect/sun.json"
import wifi from "@exponential_app/eui/eval/motion/fixtures/dialect/wifi.json"

/** A deploy card, rendered at build time: HTML + CSS, no JS. */
export const DEPLOY_CARD: NestedNode = {
  id: `card`,
  component: `Card`,
  children: [
    {
      id: `stack`,
      component: `Stack`,
      props: { gap: `md` },
      children: [
        {
          id: `head`,
          component: `Stack`,
          props: { direction: `horizontal`, justify: `between`, align: `center` },
          children: [
            { id: `title`, component: `Heading`, props: { text: `Deploy web 0.20.27`, level: 3 } },
            { id: `badge`, component: `Badge`, props: { text: `Green`, variant: `secondary` } },
          ],
        },
        {
          id: `stats`,
          component: `Grid`,
          props: { columns: 3, gap: `sm` },
          children: [
            { id: `s1`, component: `StatTile`, props: { label: `Checks`, value: `14/14` } },
            { id: `s2`, component: `StatTile`, props: { label: `Build`, value: `4m 12s` } },
            { id: `s3`, component: `StatTile`, props: { label: `Image`, value: `212 MB` } },
          ],
        },
        { id: `progress`, component: `Progress`, props: { value: 72, label: `Rolling out` } },
      ],
    },
  ],
}

/** An interactive form, hydrated on view: typing, a switch, a slider. */
export const SETTINGS_FORM: NestedNode = {
  id: `form`,
  component: `Card`,
  children: [
    {
      id: `fields`,
      component: `Stack`,
      props: { gap: `md` },
      children: [
        { id: `name`, component: `Input`, props: { label: `Board name`, value: { path: `/name` } } },
        { id: `notify`, component: `Switch`, props: { label: `Notify me on review`, checked: { path: `/notify` } } },
        { id: `wip`, component: `Slider`, props: { label: `Work-in-progress limit`, min: 1, max: 12, step: 1, value: { path: `/wip` } } },
        { id: `echo`, component: `Text`, props: { text: { path: `/name` }, variant: `muted` } },
      ],
    },
  ],
}
export const SETTINGS_DATA = { name: `Web`, notify: true, wip: 5 }

const GALLERY = [
  { ...pulseDot, title: `Live dot` },
  { ...progressRing, title: `Progress ring` },
  { ...typingDots, title: `Typing` },
  { ...heartbeat, title: `Heartbeat` },
  { ...equalizer, title: `Equalizer` },
  { ...errorX, title: `Error` },
  { ...sun, title: `Sun` },
  { ...wifi, title: `Wi-Fi` },
]

function replyJson(text: string): Record<string, unknown> {
  const m = /```json([\s\S]*?)```/.exec(text)
  return JSON.parse(m ? m[1]! : text) as Record<string, unknown>
}

/** Eight Motion tiles: WASM engine, Canvas2D paint, client-only. */
export const MOTION_GALLERY: NestedNode = {
  id: `gallery`,
  component: `Grid`,
  props: { columns: 4, gap: `md` },
  children: GALLERY.map((g) => ({
    id: `g-${g.id}`,
    component: `Stack`,
    props: { gap: `xs`, align: `center` },
    children: [
      { id: `m-${g.id}`, component: `Motion`, props: { ...replyJson(g.text), label: g.title }, style: { width: `72px`, height: `72px` } },
      { id: `t-${g.id}`, component: `Text`, props: { text: g.title, variant: `muted` } },
    ],
  })),
}

/** One small card, painted once per built-in theme on /sdk/genui/. */
export const THEME_CARD: NestedNode = {
  id: `theme-card`,
  component: `Card`,
  children: [
    {
      id: `tc-stack`,
      component: `Stack`,
      props: { gap: `sm` },
      children: [
        {
          id: `tc-head`,
          component: `Stack`,
          props: { direction: `horizontal`, justify: `between`, align: `center` },
          children: [
            { id: `tc-title`, component: `Heading`, props: { text: `Review`, level: 4 } },
            { id: `tc-badge`, component: `Badge`, props: { text: `3 open` } },
          ],
        },
        { id: `tc-progress`, component: `Progress`, props: { value: 60, label: `Review progress` } },
        {
          id: `tc-actions`,
          component: `Stack`,
          props: { direction: `horizontal`, gap: `xs` },
          children: [
            { id: `tc-approve`, component: `Button`, props: { label: `Approve` } },
            { id: `tc-later`, component: `Button`, props: { label: `Later`, variant: `outline` } },
          ],
        },
      ],
    },
  ],
}
