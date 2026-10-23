/* The concept drafts of "What a personal assistant should be able to do" (EXP-1368), written as
   A2UI trees of the core catalog and painted live by the Exponential UI React
   renderer (components/blog/LiveSurface). They are drafts: the watch view
   does not exist as product UI (EXP-1369), the voice pipeline is a plan
   (NEW-9). scripts/blog-stills.ts photographs each live render into
   public/blog/<id>.webp for the prerender, the index card and feed readers. */
import type { Nested } from "./a2ui"

const caption = (id: string, text: string): Nested => ({ id, component: `Text`, props: { text, variant: `caption` } })
const muted = (id: string, text: string): Nested => ({ id, component: `Text`, props: { text, variant: `muted` } })

const chips = (id: string, items: { label: string; tone?: string; detail?: string; icon?: string }[]): Nested => ({
  id,
  component: `Stack`,
  props: { direction: `horizontal`, gap: `xs`, align: `center`, wrap: true },
  children: items.map((item, i) => ({
    id: `${id}-${i}`,
    component: `Chip`,
    props: { label: item.label, shape: `pill`, ...(item.tone ? { tone: item.tone } : {}), ...(item.detail ? { detail: item.detail } : {}), ...(item.icon ? { icon: item.icon } : {}) },
  })),
})

const between = (id: string, text: string): Nested => ({
  id,
  component: `Stack`,
  props: { direction: `horizontal`, gap: `xs`, align: `center`, justify: `center` },
  children: [
    { id: `${id}-icon`, component: `Icon`, props: { name: `arrow-down`, size: `sm`, tone: `neutral` } },
    muted(`${id}-text`, text),
  ],
})

const layer = (id: string, title: string, description: string, status: { label: string; tone: string; detail: string }[], issues: string): Nested => ({
  id,
  component: `Card`,
  props: { title, description },
  children: [
    {
      id: `${id}-body`,
      component: `Stack`,
      props: { direction: `vertical`, gap: `sm`, align: `stretch` },
      children: [chips(`${id}-status`, status), caption(`${id}-issues`, issues)],
    },
  ],
})

/** The four layers, top to bottom. */
export const STACK_SURFACE: Nested = {
  id: `root`,
  component: `Box`,
  style: { display: `flex`, flexDirection: `column`, gap: `$spacing.md` },
  children: [
    caption(`stack-kicker`, `Four abilities of a personal assistant · one renderer underneath`),
    layer(
      `l1`,
      `1 · It shows, instead of telling`,
      `The assistant describes a screen as JSON and a renderer paints it natively: React, Solid, SwiftUI, Compose and gpui from one core. Themes are JSON, extensions add components.`,
      [
        { label: `shipped`, tone: `success`, detail: `renderers ×6, conformance, benchmarks + ratchet` },
        { label: `in progress`, tone: `warning`, detail: `Apps slice 1 ×4, the 0.1.0 release` },
      ],
      `VAPP-84 · VAPP-105 · VAPP-109 · VAPP-110`
    ),
    between(`b1`, `the same JSON on disk`),
    layer(
      `l2`,
      `2 · It shapes its environment`,
      `Panel, launcher, settings and files as templates on disk, painted by the same renderer. The assistant edits the JSON, the desktop changes live. Existing apps keep running beside it.`,
      [{ label: `planned`, tone: `neutral`, detail: `first on Mint, later a Nix-based OS` }],
      `NEW-10 · VAPP-64`
    ),
    between(`b2`, `the agent needs hands`),
    layer(
      `l3`,
      `3 · It has hands you can watch`,
      `Today: foreground, your input held, Esc takes it back. Next: work on another monitor, every run watched live or replayed as A2UI, not video.`,
      [
        { label: `shipped`, tone: `success`, detail: `foreground on Linux, macOS, Windows` },
        { label: `planned`, tone: `neutral`, detail: `the watch view, A2UI replay` },
      ],
      `EXP-1196 · EXP-1273 · EXP-1369 · VAPP-81`
    ),
    between(`b3`, `and a voice`),
    layer(
      `l4`,
      `4 · You can talk to it`,
      `Push-to-talk or wake word. Speech-to-text per audio channel on your CPU, a small router that acts, confirms or hands over, the reply spoken back.`,
      [{ label: `planned`, tone: `neutral`, detail: `call mode, per-app audio channels` }],
      `NEW-9`
    ),
  ],
}

const row = (id: string, icon: string, title: string, subtitle: string): Nested => ({
  id,
  component: `Row`,
  props: { icon, title, subtitle, surface: `card` },
})

const column = (id: string, title: string, rows: Nested[]): Nested => ({
  id,
  component: `Stack`,
  props: { direction: `vertical`, gap: `xs`, align: `stretch` },
  children: [caption(`${id}-title`, title), ...rows],
})

/** The voice pipeline: speech in, a session out, speech back. */
export const VOICE_SURFACE: Nested = {
  id: `root`,
  component: `Box`,
  style: { display: `flex`, flexDirection: `column`, gap: `$spacing.md` },
  children: [
    caption(`voice-kicker`, `Your voice is a message: local speech in, a session out, speech back`),
    {
      id: `voice-grid`,
      component: `Grid`,
      props: { columns: 2, gap: `md` },
      children: [
        column(`v1`, `1 · On your device`, [
          row(`v1-mic`, `mic`, `Mic`, `push-to-talk key or wake word`),
          row(`v1-desktop`, `audio-lines`, `Desktop audio`, `per app: Zoom, Discord`),
        ]),
        column(`v2`, `2 · Offline, on the CPU`, [
          row(`v2-stt`, `cpu`, `Whistle STT`, `16.9 MB · one pass per channel`),
          row(`v2-merge`, `layers`, `Merge by timestamps`, `you: … / Zoom: …`),
        ]),
        column(`v3`, `3 · Router · about 50 ms`, [
          row(`v3-needle`, `zap`, `Needle 3, fine-tuned`, `our tools as LoRA · confidence`),
          chips(`v3-outcomes`, [
            { label: `act`, tone: `success` },
            { label: `confirm`, tone: `warning` },
            { label: `hand over`, tone: `info` },
          ]),
          row(`v3-quick`, `keyboard`, `Quick commands`, `pause the agent · open thread 12`),
        ]),
        column(`v4`, `4 · Exponential`, [
          row(`v4-send`, `arrow-right`, `sessions.send`, `to your Assistant agent, full model`),
          row(`v4-tts`, `volume`, `TTS reply`, `speaks the final turn · caption`),
        ]),
      ],
    },
    { id: `voice-sep`, component: `Separator` },
    chips(`voice-modes`, [
      { label: `Call mode`, icon: `mic`, detail: `keep talking while the agent works on monitor 2` },
      { label: `Game-safe`, icon: `gamepad`, detail: `watch only, never input into games` },
      { label: `Per device`, icon: `monitor`, detail: `keybinds and mics stored per machine` },
    ]),
  ],
}

const step = (id: string, icon: string, title: string, description?: string, status?: string): Nested => ({
  id,
  component: `TraceItem`,
  props: { icon, title, size: `sm`, ...(description ? { description } : {}), ...(status ? { status } : {}) },
})

/** The watch view: the flow, the live window, the step graph, the replay. */
export const WATCH_SURFACE: Nested = {
  id: `root`,
  component: `Box`,
  style: { display: `flex`, flexDirection: `column`, gap: `$spacing.md` },
  children: [
    {
      id: `w-head`,
      component: `Stack`,
      props: { direction: `vertical`, gap: `xs`, align: `stretch` },
      children: [
        { id: `w-title`, component: `Heading`, props: { text: `Export last month's Stripe payouts into the Steuer board`, level: `h3` } },
        chips(`w-chips`, [
          { label: `Background · monitor 2`, tone: `success` },
          { label: `Firefox · Files` },
        ]),
      ],
    },
    {
      id: `w-actions`,
      component: `Stack`,
      props: { direction: `horizontal`, gap: `xs`, align: `center`, wrap: true },
      children: [
        { id: `w-watching`, component: `Button`, props: { label: `Watching`, icon: `eye`, variant: `secondary`, size: `sm` } },
        { id: `w-pause`, component: `Button`, props: { label: `Pause agent`, icon: `pause`, variant: `outline`, size: `sm` } },
        { id: `w-takeover`, component: `Button`, props: { label: `Take over`, icon: `hand`, variant: `destructive`, size: `sm` } },
      ],
    },
    {
      id: `w-grid`,
      component: `Grid`,
      props: { columns: 2, gap: `md` },
      children: [
        {
          id: `w-flow`,
          component: `Card`,
          props: { title: `The flow`, description: `7 steps · 2 min 14 s` },
          children: [
            {
              id: `w-trace`,
              component: `Trace`,
              props: { size: `sm` },
              children: [
                step(`s1`, `check`, `Opened Firefox`, `monitor 2 · no cursor taken`),
                step(`s2`, `check`, `Stripe, Payouts`, `AX tree · 41 elements`),
                step(`s3`, `check`, `Filter: Sep 2026`, `set value via AX`),
                step(`s4`, `pointer`, `Click Export CSV`, `window-routed click`, `running`),
                step(`s5`, `download`, `Wait for download`, undefined, `pending`),
                step(`s6`, `paperclip`, `Attach CSV to #S-1`, undefined, `pending`),
                step(`s7`, `circle-help`, `Ask before filing`, `confirm · money`, `pending`),
              ],
            },
          ],
        },
        {
          id: `w-window`,
          component: `Card`,
          props: { title: `dashboard.stripe.com/payouts`, description: `live window · captured while occluded · 4 fps` },
          children: [
            {
              id: `w-window-body`,
              component: `Stack`,
              props: { direction: `vertical`, gap: `sm`, align: `stretch` },
              children: [
                { id: `w-payouts`, component: `Heading`, props: { text: `Payouts`, level: `h4` } },
                {
                  id: `w-filters`,
                  component: `Stack`,
                  props: { direction: `horizontal`, gap: `xs`, align: `center`, wrap: true },
                  children: [
                    { id: `w-f-date`, component: `Chip`, props: { label: `Sep 1 to Sep 30, 2026`, shape: `rect` } },
                    { id: `w-f-status`, component: `Chip`, props: { label: `Paid`, shape: `rect` } },
                    { id: `w-export`, component: `Button`, props: { label: `Export CSV`, variant: `default`, size: `sm` } },
                  ],
                },
                {
                  id: `w-table`,
                  component: `Table`,
                  props: {
                    density: `compact`,
                    columns: [
                      { key: `amount`, label: `Amount` },
                      { key: `arrival`, label: `Arrival` },
                      { key: `status`, label: `Status` },
                    ],
                    rows: [
                      { amount: `€ 4,210.00`, arrival: `Sep 29`, status: `Paid` },
                      { amount: `€ 3,980.50`, arrival: `Sep 22`, status: `Paid` },
                      { amount: `€ 5,102.10`, arrival: `Sep 15`, status: `Paid` },
                      { amount: `€ 2,744.00`, arrival: `Sep 8`, status: `Paid` },
                    ],
                  },
                },
                {
                  id: `w-agent`,
                  component: `Card`,
                  props: { title: `Agent`, description: `spoken · hold F13 to reply` },
                  children: [muted(`w-agent-text`, `Exporting September payouts now. I will ask before I touch the totals.`)],
                },
              ],
            },
          ],
        },
      ],
    },
    {
      id: `w-stepper`,
      component: `Stepper`,
      props: {
        orientation: `horizontal`,
        current: 3,
        steps: [{ label: `Firefox` }, { label: `Payouts` }, { label: `Filter` }, { label: `Export` }, { label: `Download` }, { label: `Attach` }, { label: `Confirm` }],
      },
    },
    {
      id: `w-replay`,
      component: `Stack`,
      props: { direction: `vertical`, gap: `xs`, align: `stretch` },
      children: [
        {
          id: `w-replay-row`,
          component: `Stack`,
          props: { direction: `horizontal`, gap: `sm`, align: `center`, justify: `between` },
          children: [
            { id: `w-play`, component: `Button`, props: { label: `Replay`, icon: `play`, variant: `outline`, size: `sm` } },
            { id: `w-time`, component: `Text`, props: { text: `01:23 / 02:14 · A2UI replay, 3 KB`, variant: `code` } },
          ],
        },
        { id: `w-scrub`, component: `Progress`, props: { value: 83, max: 134, tone: `primary` } },
      ],
    },
  ],
}
