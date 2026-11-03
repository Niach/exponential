/* Libraries: each one Overview / Guides / Reference. eUI and Motion list
   their pages by hand (eUI's components come from the catalog, by group);
   the other libraries keep the generic three. */
import componentsDoc from "@exponential_app/eui/docs/components.generated.json"
import { LIBRARIES } from "../lib/libraries.ts"
import { EUI_GROUP_LABELS, EUI_GROUP_ORDER, euiComponentPath } from "../lib/eui-components.ts"

const EUI = `/docs/libraries/eui`
const MOTION = `/docs/libraries/motion`

/** The eUI guides in reading order (slug → sidebar label). */
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

const componentGroups = EUI_GROUP_ORDER.map((group) => ({
  label: EUI_GROUP_LABELS[group] ?? group,
  collapsed: true,
  items: componentsDoc.components.filter((c) => c.group === group).map((c) => ({ label: c.name, link: euiComponentPath(c) })),
})).filter((g) => g.items.length > 0)

const eui = {
  label: `eUI`,
  collapsed: false,
  items: [
    { label: `Overview`, link: `${EUI}/` },
    {
      label: `Guides`,
      collapsed: false,
      items: [{ label: `All guides`, link: `${EUI}/guides/` }, ...EUI_GUIDES.map(([slug, label]) => ({ label, link: `${EUI}/guides/${slug}/` }))],
    },
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

export const LIBRARIES_GROUP = {
  label: `Libraries`,
  items: LIBRARIES.map((lib) => {
    if (lib.slug === `eui`) return eui
    if (lib.slug === `motion`) return motion
    return {
      label: lib.name,
      collapsed: true,
      items: [
        { label: `Overview`, link: `/docs/libraries/${lib.slug}/` },
        { label: `Guides`, link: `/docs/libraries/${lib.slug}/guides/` },
        { label: `Reference`, link: `/docs/libraries/${lib.slug}/reference/` },
      ],
    }
  }),
}
