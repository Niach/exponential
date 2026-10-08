// VAPP-87: the theme builder's PREVIEW, painted by the real React renderer
// (`@exponential-at/ui-react`): the recipe sheet (every component part × its
// recipe props, the interaction states forced per column) and the kitchen
// sink, re-rendered on every draft edit. The VAPP-92 stand-in DOM painter is
// gone; what the builder shows IS what a host gets.

import { createElement, Fragment } from "react"
import { createRoot, type Root } from "react-dom/client"
import * as lucide from "lucide-react"
import { ExponentialSurface } from "@exponential-at/ui-react"
import type { IconComponent, IconMap } from "@exponential-at/ui-react"
import { CORE_CATALOG_ID } from "../src/catalog"
import { reduceNested } from "../src/reducer"
import type { ModeName, ResolvedTheme } from "../src/theme-types"
import type { NestedNode } from "../src/types"
import kitchenSink from "../fixtures/kitchen-sink.json" with { type: "json" }
import registry from "../../icons/icons.json" with { type: "json" }

const pascal = (name: string) => name.replace(/(^|-)([a-z0-9])/g, (_, __, c: string) => c.toUpperCase())
const semantic = (registry as { semantic: Record<string, string> }).semantic
/** Lucide by registry name or concept id (the app's custom glyphs fall back
 *  to the placeholder; the builder is a dev page, not the app). */
const icons: IconMap = (name: string): IconComponent | undefined => (lucide as unknown as Record<string, IconComponent | undefined>)[pascal(semantic[name] ?? name)]

export interface PreviewState {
  theme: ResolvedTheme
  mode: ModeName
  width: number
}

export const SHEET: { title: string; nodes: NestedNode[]; states?: string[][] }[] = [
  {
    title: `Button · variant × size`,
    nodes: [`default`, `secondary`, `outline`, `ghost`, `destructive`, `link`].flatMap((variant) => [`sm`, `default`, `lg`, `icon`].map((size) => ({ id: `b-${variant}-${size}`, component: `Button`, props: { label: variant, icon: size === `icon` ? `ui-add` : undefined, variant, size } }))),
  },
  {
    title: `Button · states`,
    nodes: [`default`, `outline`].map((variant) => ({ id: `bs-${variant}`, component: `Button`, props: { label: variant, variant } })),
    states: [[], [`hover`], [`pressed`], [`focus`], [`disabled`]],
  },
  { title: `Badge`, nodes: [`default`, `secondary`, `outline`, `ghost`, `destructive`, `link`].map((variant) => ({ id: `badge-${variant}`, component: `Badge`, props: { text: variant, icon: `ui-check`, variant } })) },
  { title: `Pill · tone, selected`, nodes: [...[`neutral`, `primary`, `success`, `warning`, `danger`, `info`].map((tone) => ({ id: `pill-${tone}`, component: `Pill`, props: { label: tone, tone, icon: `ui-check` } })), { id: `pill-sel`, component: `Pill`, props: { label: `selected`, selected: true } }] },
  { title: `Text`, nodes: [`body`, `caption`, `muted`, `lead`, `code`, `label`, `title`].map((variant) => ({ id: `text-${variant}`, component: `Text`, props: { text: variant, variant } })) },
  { title: `Inputs`, nodes: [{ id: `in-1`, component: `Input`, props: { label: `Email`, name: `email`, placeholder: `you@example.com` } }, { id: `in-2`, component: `Select`, props: { label: `Board`, name: `board`, options: [{ label: `Sprint`, value: `sprint` }], value: `sprint` } }, { id: `in-3`, component: `Textarea`, props: { label: `Notes`, name: `notes`, placeholder: `Anything else?` } }] },
  { title: `Toggles`, nodes: [{ id: `t-1`, component: `Switch`, props: { label: `Auto-scan`, name: `a`, checked: true } }, { id: `t-2`, component: `Switch`, props: { label: `Off`, name: `b`, checked: false } }, { id: `t-3`, component: `Checkbox`, props: { label: `Checked`, name: `c`, checked: true } }, { id: `t-4`, component: `Checkbox`, props: { label: `Unchecked`, name: `d`, checked: false } }, { id: `t-5`, component: `Radio`, props: { name: `r`, options: [{ label: `One`, value: `1` }, { label: `Two`, value: `2` }], value: `1` } }, { id: `t-6`, component: `Toggle`, props: { label: `Bold`, icon: `editor-bold`, pressed: true } }, { id: `t-7`, component: `Toggle`, props: { label: `Outline`, variant: `outline` } }] },
  { title: `Navigation`, nodes: [{ id: `n-1`, component: `Tabs`, props: { tabs: [{ label: `Issue`, value: `issue` }, { label: `Run`, value: `run` }, { label: `Changes`, value: `changes` }], value: `issue` } }, { id: `n-2`, component: `ToggleGroup`, props: { items: [{ label: `List`, value: `list` }, { label: `Board`, value: `board` }], value: `list`, variant: `segmented` } }, { id: `n-3`, component: `ToggleGroup`, props: { items: [{ label: `A`, value: `a` }, { label: `B`, value: `b` }], value: `a`, variant: `outline` } }, { id: `n-4`, component: `Pagination`, props: { page: 2, totalPages: 5 } }] },
  { title: `Feedback`, nodes: [...[`info`, `success`, `warning`, `error`].map((type) => ({ id: `al-${type}`, component: `Alert`, props: { type, title: `${type} alert`, message: `Something to know.` } })), { id: `pr-1`, component: `Progress`, props: { value: 62, label: `Upload` } }, { id: `me-1`, component: `Meter`, props: { label: `Usage`, segments: [{ value: 40, tone: `success` }, { value: 25, tone: `warning` }, { value: 10, tone: `danger` }] } }, { id: `ri-1`, component: `Ring`, props: { value: 0.62 } }, { id: `sp-1`, component: `Spinner`, props: {} }, { id: `sk-1`, component: `Skeleton`, props: { width: `120px`, height: `16px` } }] },
  { title: `Lists`, nodes: [{ id: `l-band`, component: `Band`, props: { title: `In progress`, count: 3, icon: `ui-check` } }, { id: `l-row`, component: `ListRow`, props: { title: `A list row`, subtitle: `with a subtitle`, meta: `2d`, icon: `ui-check`, chevron: true } }, { id: `l-row-sel`, component: `ListRow`, props: { title: `Selected row`, selected: true } }, { id: `l-card`, component: `CardRow`, props: { title: `Card row`, subtitle: `bordered`, icon: `ui-check` } }, { id: `l-nav`, component: `NavRow`, props: { label: `Inbox`, icon: `nav-inbox`, count: 4, selected: true } }, { id: `l-chip`, component: `EntityChip`, props: { label: `APP-12`, detail: `Quick-add`, icon: `ui-check`, removable: true } }] },
  { title: `Surfaces`, nodes: [{ id: `s-card`, component: `Card`, props: { title: `Card`, description: `A card with a body`, padded: true }, children: [{ id: `s-card-t`, component: `Text`, props: { text: `Body text` } }] }, { id: `s-group`, component: `Group`, props: { title: `Group`, footer: `Footer note` }, children: [{ id: `s-g-1`, component: `PropertyRow`, props: { label: `Status`, value: `Open` } }, { id: `s-g-2`, component: `PickerRow`, props: { label: `Assignee`, placeholder: `Choose` } }] }, { id: `s-tip`, component: `Tooltip`, props: { content: `Tooltip` }, children: [{ id: `s-tip-b`, component: `Button`, props: { label: `Hover`, variant: `outline` } }] }, { id: `s-pop`, component: `Popover`, props: { open: false }, slots: { trigger: { id: `s-pop-t`, component: `Button`, props: { label: `Open`, variant: `secondary` } } }, children: [{ id: `s-pop-c`, component: `Text`, props: { text: `Popover content` } }] }, { id: `s-menu`, component: `DropdownMenu`, props: { items: [{ label: `Edit`, value: `edit`, icon: `ui-edit` }, { label: `Share`, value: `share` }, { separator: true }, { label: `Delete`, value: `delete`, destructive: true }] }, slots: { trigger: { id: `s-menu-t`, component: `Button`, props: { label: `Menu`, variant: `outline` } } } }, { id: `s-dialog`, component: `Dialog`, props: { open: false, title: `Dialog`, description: `A modal surface` }, children: [{ id: `s-dialog-c`, component: `Text`, props: { text: `Dialog body` } }], slots: { trigger: { id: `s-dialog-t`, component: `Button`, props: { label: `Open dialog` } }, footer: { id: `s-dialog-f`, component: `Button`, props: { label: `Confirm` } } } }] },
]

const h = createElement

function Preview({ theme, mode, width }: PreviewState) {
  const host = { icons }
  const surface = (id: string, node: NestedNode, states: string[] = []) => {
    const { root } = reduceNested(node, { catalogId: CORE_CATALOG_ID })
    return h(ExponentialSurface, { key: id, id, root, theme, mode, host, states })
  }
  const sheet = h(
    `div`,
    { className: `sheet` },
    ...SHEET.map((group, gi) =>
      h(
        `section`,
        { key: gi, className: `sheet-group` },
        h(`h3`, null, group.title),
        h(
          `div`,
          { className: `sheet-row` },
          ...(group.states ?? [[]]).map((states, si) =>
            h(
              `div`,
              { key: si, className: `sheet-col` },
              group.states ? h(`div`, { className: `sheet-state` }, states[0] ?? `default`) : null,
              ...group.nodes.map((node) => surface(`${gi}-${si}-${node.id}`, node, states))
            )
          )
        )
      )
    )
  )
  const { root } = reduceNested(kitchenSink as unknown as NestedNode, { catalogId: CORE_CATALOG_ID })
  const sink = h(`section`, { className: `sink` }, h(`h3`, null, `Kitchen sink · exponential-ui-kitchen-sink`), h(ExponentialSurface, { id: `sink`, root, theme, mode, host, width }))
  return h(Fragment, null, sheet, sink)
}

let reactRoot: Root | null = null
let reactHost: HTMLElement | null = null

/** Render (or re-render) the preview into `host`. */
export function renderPreview(host: HTMLElement, state: PreviewState): void {
  if (reactHost !== host) {
    reactRoot?.unmount()
    reactRoot = createRoot(host)
    reactHost = host
  }
  reactRoot!.render(h(Preview, state))
}
