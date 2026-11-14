/* The SDK: each module Overview / Guides / Reference. genUI and Motion list
   their pages by hand (genUI's components come from the catalog, by group);
   the other modules keep the generic three. */
import componentsDoc from "@exponential_app/eui/docs/components.generated.json"
import { EAPPS } from "./eapps.ts"
import { LIBRARIES } from "../lib/libraries.ts"
import { EUI_GROUP_LABELS, EUI_GROUP_ORDER, euiComponentPath } from "../lib/eui-components.ts"

const EUI = `/docs/sdk/genui`
const MOTION = `/docs/sdk/motion`

/** The genUI guides in reading order (slug → sidebar label). */
export const EUI_GUIDES: [string, string][] = [
  [`react`, `React`],
  [`solid`, `Solid`],
  [`element`, `Web component`],
  [`swiftui`, `SwiftUI`],
  [`compose`, `Compose`],
  [`gpui`, `gpui`],
  [`define`, `Surfaces in TypeScript`],
  [`themes`, `Themes`],
  [`extensions`, `Extensions`],
  [`host-plugins`, `Host plugins`],
  [`agents`, `Agents`],
]

/** The question-style explainers and the comparisons (slug → sidebar label). */
export const EUI_LEARN: [string, string][] = [
  [`learn/what-is-a2ui`, `What is A2UI?`],
  [`learn/native-rendering`, `Native rendering`],
  [`learn/theming`, `Theming at runtime`],
  [`learn/a2ui-over-mcp`, `A2UI over MCP`],
  [`compare/a2ui-vs-server-driven-ui`, `A2UI vs server-driven UI`],
  [`compare/generative-ui-vs-native-code`, `vs hand-written native`],
]

const componentGroups = EUI_GROUP_ORDER.map((group) => ({
  label: EUI_GROUP_LABELS[group] ?? group,
  collapsed: true,
  items: componentsDoc.components.filter((c) => c.group === group).map((c) => ({ label: c.name, link: euiComponentPath(c) })),
})).filter((g) => g.items.length > 0)

const eui = {
  label: `genUI`,
  collapsed: false,
  items: [
    { label: `Overview`, link: `${EUI}/` },
    {
      label: `Guides`,
      collapsed: false,
      items: [{ label: `All guides`, link: `${EUI}/guides/` }, ...EUI_GUIDES.map(([slug, label]) => ({ label, link: `${EUI}/guides/${slug}/` }))],
    },
    { label: `Learn`, collapsed: true, items: EUI_LEARN.map(([slug, label]) => ({ label, link: `${EUI}/${slug}/` })) },
    {
      label: `Reference`,
      collapsed: true,
      items: [
        { label: `Overview`, link: `${EUI}/reference/` },
        { label: `Components`, collapsed: true, items: [{ label: `All components`, link: `${EUI}/reference/components/` }, ...componentGroups] },
        { label: `Themes`, link: `${EUI}/reference/themes/` },
        { label: `Conformance`, link: `${EUI}/reference/conformance/` },
        { label: `Numbers`, link: `${EUI}/reference/numbers/` },
        { label: `Ergonomics eval`, link: `${EUI}/reference/eval/` },
        { label: `Security`, link: `${EUI}/reference/security/` },
      ],
    },
    {
      label: `Tools`,
      collapsed: true,
      items: [
        { label: `Playground`, link: `${EUI}/tools/playground/` },
        { label: `Theme builder`, link: `${EUI}/tools/theme-builder/` },
      ],
    },
  ],
}

const motion = {
  label: `Motion`,
  collapsed: true,
  items: [
    { label: `Overview`, link: `${MOTION}/` },
    { label: `Guides`, collapsed: false, items: [{ label: `Animate with Motion`, link: `${MOTION}/guides/animate/` }] },
    {
      label: `Reference`,
      collapsed: false,
      items: [
        { label: `Specification`, link: `${MOTION}/reference/` },
        { label: `Engine and players`, link: `${MOTION}/reference/engine/` },
        { label: `Dialect`, link: `${MOTION}/reference/dialect/` },
      ],
    },
  ],
}

/** The other modules' extra guides and references (slug → page slugs under guides/ and reference/). */
const EXTRA: Record<string, { guides?: [string, string][]; reference?: [string, string][] }> = {
  voice: {
    guides: [[`install`, `Install`], [`your-voice`, `Your voice`], [`push-to-talk`, `Push to talk`]],
    reference: [[`api`, `API`], [`numbers`, `Numbers`], [`licences`, `Licences`], [`contract`, `Contract`], [`gpui`, `gpui`], [`ffi`, `FFI`], [`wakeword`, `Wake word`]],
  },
  p2p: { reference: [[`ffi`, `FFI`], [`wasm`, `WASM`]] },
  "computer-use": { reference: [[`screen`, `Screen`]] },
  "mcp-proxy": { reference: [[`contract`, `Contract`]] },
  agents: { reference: [[`orchestration`, `Orchestration`], [`hub-core`, `Hub core`], [`hub-core-wasm`, `Hub core WASM`], [`migrating`, `Migrating`]] },
  core: { reference: [[`contract`, `Contract`]] },
  code: { reference: [[`contract`, `Contract`]] },
}

function otherLibrary(slug: string, name: string) {
  const base = `/docs/sdk/${slug}`
  const extra = EXTRA[slug] ?? {}
  const section = (label: string, dir: string, pages?: [string, string][]) =>
    pages?.length
      ? { label, collapsed: true, items: [{ label: `Overview`, link: `${base}/${dir}/` }, ...pages.map(([p, l]) => ({ label: l, link: `${base}/${dir}/${p}/` }))] }
      : { label, link: `${base}/${dir}/` }
  return {
    label: name,
    collapsed: true,
    items: [{ label: `Overview`, link: `${base}/` }, section(`Guides`, `guides`, extra.guides), section(`Reference`, `reference`, extra.reference)],
  }
}

export const LIBRARIES_GROUP = {
  label: `SDK`,
  items: [
    { label: `Overview`, link: `/docs/sdk/` },
    { label: `Install from a release tag`, link: `/docs/sdk/install/` },
    ...LIBRARIES.map((lib) => {
      if (lib.slug === `genui`) return eui
      if (lib.slug === `motion`) return motion
      return otherLibrary(lib.slug, lib.name)
    }),
    EAPPS,
  ],
}
