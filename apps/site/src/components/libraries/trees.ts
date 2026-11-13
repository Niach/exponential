/* The libraries' product-page visuals as genUI trees: rendered at build time
   by EuiSurface (HTML + CSS, no JS), so every diagram is drawn with the same
   components the apps use. A node = icon + short title; an arrow = an Icon. */
import type { NestedNode } from "@exponential_app/eui"

type Node = NestedNode

const responsive = { base: `vertical`, md: `horizontal` } as const

/** One box of a diagram: an icon over a short title. */
export function box(id: string, icon: string, title: string): Node {
  return {
    id,
    component: `Card`,
    props: { style: { flex: 1 } },
    children: [
      {
        id: `${id}-s`,
        component: `Stack`,
        props: { gap: `sm`, align: `center` },
        children: [
          { id: `${id}-i`, component: `Icon`, props: { name: icon, size: `lg` } },
          { id: `${id}-t`, component: `Text`, props: { text: title, variant: `label`, align: `center` } },
        ],
      },
    ],
  }
}

/** An arrow between boxes (right on wide screens, down on narrow ones is fine: it reads as "then"). */
export function arrow(id: string, icon = `arrow-right`): Node {
  return { id, component: `Icon`, props: { name: icon, tone: `neutral`, label: `to` } }
}

/** Boxes in a row with arrows between them. */
export function flow(id: string, items: Node[], connector = `arrow-right`): Node {
  const children: Node[] = []
  items.forEach((item, i) => {
    if (i) children.push(arrow(`${id}-a${i}`, connector))
    children.push(item)
  })
  return { id, component: `Stack`, props: { direction: responsive, gap: `md`, align: `center` }, children }
}

/** Boxes in an even grid. */
export function grid(id: string, items: Node[], columns: number): Node {
  return { id, component: `Grid`, props: { columns: { base: 2, md: columns }, gap: `md` }, children: items }
}

/** A column of boxes. */
function column(id: string, items: Node[]): Node {
  return { id, component: `Stack`, props: { gap: `sm`, style: { flex: 1 } }, children: items }
}

/* Voice ------------------------------------------------------------------ */

/** The push-to-talk capsule through its three states (voice.json `overlaySpecimens`). */
export const TALK_CAPSULES: Node = {
  id: `capsules`,
  component: `Stack`,
  props: { gap: `sm`, align: `center` },
  children: [
    [`listening`, 0.7, [0.9, 0.6, 1, 0.5], `Listening`, `“create an issue push to talk breaks under Wayland”`, `Studio`, false],
    [`thinking`, 0, [], `Heard`, `“create an issue push to talk breaks under Wayland”`, `Studio`, false],
    [`speaking`, 0.5, [0.7, 1, 0.5, 0.8], `Speaking`, `Created NEW-81 “Push to talk breaks under Wayland”.`, `Stop`, true],
  ].map(([activity, level, bands, state, line, pill, primary], i) => ({
    id: `cap${i}`,
    component: `Card`,
    props: { style: { width: `100%`, maxWidth: `36rem` } },
    children: [
      {
        id: `cap${i}-r`,
        component: `Stack`,
        props: { direction: `horizontal`, gap: `sm`, align: `center` },
        children: [
          { id: `cap${i}-w`, component: `VoiceWave`, props: { activity, level, bands, size: `md`, label: `Voice` } },
          { id: `cap${i}-s`, component: `Text`, props: { text: state, variant: `label` } },
          { id: `cap${i}-l`, component: `Text`, props: { text: line, variant: `muted`, lines: 1, style: { flex: 1 } } },
          { id: `cap${i}-p`, component: `Button`, props: { label: pill, size: `sm`, variant: primary ? `default` : `secondary` } },
        ],
      },
    ],
  })) as Node[],
}

/** Measured numbers (docs: /docs/sdk/voice/reference/numbers/). */
export const TALK_NUMBERS: Node = grid(
  `talk-numbers`,
  [
    { id: `n1`, component: `StatTile`, props: { label: `Text after you let go`, value: `10–41 ms` } },
    { id: `n2`, component: `StatTile`, props: { label: `Words while you speak`, value: `every 300 ms` } },
    { id: `n3`, component: `StatTile`, props: { label: `Languages`, value: `English, Deutsch` } },
    { id: `n4`, component: `StatTile`, props: { label: `Your voice from`, value: `8–15 s` } },
  ],
  4
)

/* P2P ------------------------------------------------------------------- */

export const NET_PATHS: Node = {
  id: `net`,
  component: `Stack`,
  props: { gap: `lg` },
  children: [
    flow(`net-direct`, [box(`n-phone`, `smartphone`, `Phone`), box(`n-desk`, `monitor`, `Desktop`)], `arrow-left-right`),
    flow(`net-relay`, [box(`n-web`, `globe`, `Browser`), box(`n-relay`, `radio-tower`, `Relay`), box(`n-desk2`, `monitor`, `Desktop`)], `arrow-left-right`),
  ],
}

export const NET_CHANNELS: Node = grid(
  `channels`,
  [
    box(`c-chat`, `message-square`, `Chat`),
    box(`c-files`, `file`, `Files`),
    box(`c-records`, `database`, `Records`),
    box(`c-calls`, `phone`, `Calls`),
    box(`c-push`, `bell`, `Push`),
  ],
  5
)

/* Computer Use ----------------------------------------------------------------- */

export const PILOT_LOOP: Node = flow(`loop`, [
  box(`p-observe`, `eye`, `Observe`),
  box(`p-plan`, `brain`, `Plan`),
  box(`p-act`, `mouse-pointer-click`, `Act`),
  box(`p-again`, `eye`, `Observe again`),
])

export const PILOT_PLATFORMS: Node = grid(
  `os`,
  [box(`os-mac`, `laptop`, `macOS`), box(`os-win`, `monitor`, `Windows`), box(`os-x11`, `terminal`, `Linux X11`)],
  3
)

/* MCP Proxy ------------------------------------------------------------------- */

export const MCP_HUB: Node = {
  id: `hub`,
  component: `Stack`,
  props: { direction: responsive, gap: `md`, align: `center` },
  children: [
    column(`hub-in`, [box(`h-claude`, `terminal`, `Claude Code`), box(`h-codex`, `terminal`, `Codex`)]),
    arrow(`hub-a1`),
    box(`h-hub`, `shield-check`, `One endpoint, one token per run`),
    arrow(`hub-a2`),
    column(`hub-out`, [box(`h-own`, `wrench`, `Your tools`), box(`h-up`, `server`, `Upstream servers`), box(`h-code`, `braces`, `Code mode`)]),
  ],
}

/* Agents -------------------------------------------------------------- */

export const AGENT_FLOW: Node = flow(`agent`, [
  box(`a-app`, `app-window`, `Your app`),
  box(`a-eagent`, `cpu`, `Agents`),
  box(`a-cli`, `terminal`, `claude · codex`),
])

/* Core ------------------------------------------------------------------ */

export const CORE_MODULES: Node = {
  id: `core`,
  component: `Stack`,
  props: { gap: `md`, align: `stretch` },
  children: [
    grid(
      `core-mods`,
      [
        box(`m-talk`, `mic`, `Voice`),
        box(`m-pilot`, `mouse-pointer-click`, `Computer Use`),
        box(`m-net`, `network`, `P2P`),
        box(`m-agent`, `cpu`, `Agents`),
        box(`m-mcp`, `plug`, `MCP Proxy`),
      ],
      5
    ),
    { id: `core-down`, component: `Stack`, props: { align: `center` }, children: [arrow(`core-a`, `arrow-down`)] },
    box(`m-core`, `boxes`, `Core`),
  ],
}

export const CORE_TYPES: Node = grid(
  `types`,
  [
    box(`t-ids`, `fingerprint`, `Ids`),
    box(`t-grants`, `key-round`, `Grants`),
    box(`t-caps`, `shield`, `Capabilities`),
    box(`t-verdict`, `circle-check`, `Delivery verdict`),
    box(`t-errors`, `circle-alert`, `Errors`),
    box(`t-env`, `mail`, `Event envelope`),
  ],
  3
)

export const CORE_LANGUAGES: Node = grid(
  `langs`,
  [box(`l-rust`, `cog`, `Rust`), box(`l-ts`, `file-code`, `TypeScript`), box(`l-swift`, `smartphone`, `Swift`), box(`l-kotlin`, `smartphone`, `Kotlin`)],
  4
)

/* Code ------------------------------------------------------------------ */

/** The providers behind one interface (packages/ecode `capability.byProvider`). */
export const CODE_PROVIDERS: Node = {
  id: `code`,
  component: `Stack`,
  props: { gap: `md`, align: `stretch` },
  children: [
    box(`c-lib`, `git-branch`, `Remote · Forge · Ci`),
    { id: `code-down`, component: `Stack`, props: { align: `center` }, children: [arrow(`code-a`, `arrow-down`)] },
    grid(`c-providers`, [box(`c-github`, `github`, `GitHub`), box(`c-server`, `server`, `Our own, on the server`), box(`c-local`, `hard-drive`, `Our own, no server`)], 3),
  ],
}

/** A PR tree from open to landed (forge.json). */
export const CODE_TREE: Node = flow(`tree`, [
  box(`t-open`, `git-pull-request`, `Open on any base`),
  box(`t-checks`, `list-checks`, `Checks`),
  box(`t-merge`, `git-merge`, `Squash merge`),
  box(`t-retarget`, `git-compare-arrows`, `Children retargeted`),
])

/** Own CI: the pipeline file, the planner, runners on team devices (pipeline.json, runner.json). */
export const CODE_CI: Node = flow(`ci`, [
  box(`ci-file`, `workflow`, `.exponential/ci.json`),
  box(`ci-plan`, `boxes`, `Planned jobs`),
  box(`ci-run`, `play`, `Your devices run them`),
  box(`ci-checks`, `list-checks`, `Check rows`),
])

/* Apps ------------------------------------------------------------------ */

export const APPS_PARTS: Node = grid(
  `parts`,
  [
    box(`pt-manifest`, `braces`, `manifest`),
    box(`pt-templates`, `layout-template`, `templates`),
    box(`pt-bindings`, `plug`, `bindings`),
    box(`pt-records`, `database`, `records`),
    box(`pt-functions`, `shield-check`, `functions`),
    box(`pt-exposes`, `wrench`, `exposes`),
    box(`pt-upstreams`, `server`, `upstreams`),
  ],
  4
)

export const APPS_EXECUTORS: Node = grid(
  `exec`,
  [box(`x-client`, `monitor`, `Client host`), box(`x-broker`, `server`, `Server broker`)],
  2
)

export const APPS_FLOW: Node = {
  id: `apps-flow`,
  component: `Stack`,
  props: { direction: responsive, gap: `md`, align: `center` },
  children: [
    column(`af-in`, [box(`af-records`, `database`, `Records`), box(`af-up`, `server`, `Upstreams`)]),
    arrow(`af-a1`),
    box(`af-app`, `sparkles`, `Your app`),
    arrow(`af-a2`),
    column(`af-out`, [box(`af-team`, `users`, `Your team`), box(`af-tools`, `wrench`, `MCP tools`)]),
  ],
}

export const APPS_PLATFORMS: Node = grid(
  `apps-platforms`,
  [box(`ap-web`, `globe`, `Web`), box(`ap-desk`, `laptop`, `Desktop`), box(`ap-ios`, `smartphone`, `iOS`), box(`ap-android`, `smartphone`, `Android`)],
  4
)

/** A call with the voice device's agent (voice-call.json's states), ringing and talking. */
export const TALK_CALL: Node = grid(
  `talk-call`,
  [
    { id: `call-ring`, component: `CallSurface`, props: { name: `Assistant`, state: `ringing`, status: `Calling…` } },
    {
      id: `call-talk`,
      component: `CallSurface`,
      props: { name: `Assistant`, state: `talking`, activity: `speaking`, level: 0.6, bands: [0.7, 1, 0.5, 0.8], status: `04:12`, caption: `Created NEW-81 “Push to talk breaks under Wayland”.` },
    },
  ],
  2
)
