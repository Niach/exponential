// VAPP-85: the catalog's own invariants — the ones the issue states (shadcn
// variants and sizes, natives vs macros, the lite subset, one description per
// component and per prop, every basic component mapped) and the ones the
// generator relies on (enums and shapes resolve, examples validate).

import { describe, expect, test } from "bun:test"
import {
  A2UI_BASIC_CATALOG_ID,
  CORE_CATALOG_ID,
  CORE_LITE_CATALOG_ID,
  SUPPORTED_CATALOG_IDS,
  TOKEN_GROUPS,
  componentNames,
  coreCatalog,
  coreLite,
  coreMacros,
} from "./catalog"
import { basicMap } from "./basic-map"
import { ICON_NAMES, LITE_COMPONENTS, MACRO_COMPONENTS, NATIVE_COMPONENTS, SPECIMEN_IDS } from "./catalog.generated"
import { validateProps } from "./validate"
import { CORE_FUNCTIONS } from "./expr"
import vendoredBasic from "../vendor/a2ui/v0_9/basic_catalog.json" with { type: "json" }

const components = Object.entries(coreCatalog.components)

describe(`catalog ids`, () => {
  test(`the ids the issue names`, () => {
    expect(CORE_CATALOG_ID).toBe(`https://ui.exponential.at/catalogs/core/v1`)
    expect(CORE_LITE_CATALOG_ID).toBe(`https://ui.exponential.at/catalogs/core-lite/v1`)
    expect(A2UI_BASIC_CATALOG_ID).toBe(`https://a2ui.org/specification/v0_9/basic_catalog.json`)
    expect(SUPPORTED_CATALOG_IDS).toEqual([CORE_CATALOG_ID, CORE_LITE_CATALOG_ID, A2UI_BASIC_CATALOG_ID])
  })

  test(`the vendored basic catalog carries the id the map starts from`, () => {
    expect((vendoredBasic as { catalogId: string }).catalogId).toBe(A2UI_BASIC_CATALOG_ID)
  })
})

describe(`components`, () => {
  test(`the component table is in the catalog; the round-3 folds are gone (round 4: no aliases)`, () => {
    const expected = [
      `Box`, `Stack`, `Grid`, `Card`, `Separator`, `List`, `Heading`, `Text`, `Image`, `Icon`, `Video`, `AudioPlayer`,
      `Avatar`, `Badge`, `Alert`, `EmptyState`, `Table`, `Carousel`, `Tabs`, `Accordion`,
      `Collapsible`, `Pagination`, `Dialog`, `Drawer`, `Popover`, `Tooltip`,
      `Progress`, `Meter`, `Ring`, `Spinner`, `Skeleton`, `Button`, `Link`, `Toggle`, `Input`, `Textarea`, `Checkbox`,
      `Radio`, `Switch`, `Slider`, `Select`, `DatePicker`, `Group`, `Markdown`, `Composer`, `TreeGuides`, `Chart`,
    ]
    // Round 1 (docs/round-1-contract.md §3): the renderer-hardening additions.
    const round1 = [
      `ScrollArea`, `Sidebar`, `AppBar`, `CodeBlock`, `Kbd`, `Label`, `Breadcrumb`, `Stepper`, `AlertDialog`,
      `Toast`, `Sparkline`, `Form`, `NumberField`, `Rating`, `ChipInput`, `DateRangePicker`,
      `TimePicker`, `FileUpload`,
    ]
    // Round 2 (docs/round-2-contract.md §1).
    const round2 = [`Resizable`]
    // Round 3 (docs/round-3-contract.md): the app's list / menu / chip vocabulary.
    const round3 = [`Row`, `Section`, `Chip`, `Segmented`, `Menu`]
    // Round 4 (VAPP-103): the names round 3 folded are REMOVED, not aliased.
    const removed = [`Band`, `RowList`, `ToggleGroup`, `ButtonGroup`, `TabBar`, `Sheet`, `HoverCard`, `DropdownMenu`, `ContextMenu`, `Pill`, `ListRow`, `CardRow`, `PropertyRow`, `PickerRow`, `NavRow`, `EntityChip`]
    for (const name of [...expected, ...round1, ...round2, ...round3]) expect(coreCatalog.components[name], name).toBeDefined()
    for (const name of removed) expect(coreCatalog.components[name], name).toBeUndefined()
    for (const def of Object.values(coreCatalog.components)) expect(`deprecated` in def).toBe(false)
    const hidden = [`TreeGuides`] // round 3: a Row part, never authored
    expect(Object.keys(coreCatalog.components)).toHaveLength(expected.length + round1.length + round2.length + round3.length + 1) // + the Unknown placeholder
    expect(componentNames()).toHaveLength(expected.length + round1.length + round2.length + round3.length - hidden.length)
  })

  test(`kinds follow the issue's table`, () => {
    const kind = (name: string) => coreCatalog.components[name].kind
    for (const native of [`Box`, `List`, `Text`, `Image`, `Icon`, `Video`, `AudioPlayer`, `Avatar`, `Carousel`, `Tabs`, `Segmented`, `Accordion`, `Dialog`, `Drawer`, `Popover`, `Tooltip`, `Menu`, `Ring`, `Spinner`, `Skeleton`, `Button`, `Link`, `Toggle`, `Input`, `Textarea`, `Checkbox`, `Radio`, `Switch`, `Slider`, `Select`, `DatePicker`, `Markdown`, `Composer`, `TreeGuides`, `Chart`])
      expect(kind(native), native).toBe(`native`)
    for (const macro of [`Stack`, `Grid`, `Card`, `Separator`, `Heading`, `Badge`, `Alert`, `EmptyState`, `Pagination`, `Progress`, `Meter`, `Group`, `Collapsible`, `Row`, `Section`, `Chip`])
      expect(kind(macro), macro).toBe(`macro`)
    // Round 1: natives only where a renderer must own behaviour, macros elsewhere.
    for (const native of [`Table`, `Form`, `NumberField`, `ChipInput`, `DateRangePicker`, `TimePicker`, `FileUpload`, `CodeBlock`, `Toast`])
      expect(kind(native), native).toBe(`native`)
    for (const macro of [`ScrollArea`, `Sidebar`, `AppBar`, `Kbd`, `Label`, `Breadcrumb`, `Stepper`, `AlertDialog`, `Sparkline`, `Rating`])
      expect(kind(macro), macro).toBe(`macro`)
    expect(([...NATIVE_COMPONENTS] as string[]).sort()).toEqual(components.filter(([, d]) => d.kind === `native`).map(([n]) => n).sort())
    expect(([...MACRO_COMPONENTS] as string[]).sort()).toEqual(components.filter(([, d]) => d.kind === `macro`).map(([n]) => n).sort())
  })

  test(`every macro component has a template and every template a macro component`, () => {
    const macroComponents = components.filter(([, d]) => d.kind === `macro`).map(([n]) => n).sort()
    expect(Object.keys(coreMacros).sort()).toEqual(macroComponents)
    for (const def of Object.values(coreMacros)) expect(def.root.part).toBe(`root`)
  })

  test(`shadcn's variant and size vocabularies`, () => {
    expect([...coreCatalog.enums.variant]).toEqual([`default`, `secondary`, `outline`, `ghost`, `destructive`, `link`])
    expect([...coreCatalog.enums.size]).toEqual([`sm`, `default`, `lg`, `icon`])
    expect(coreCatalog.components.Button.props.variant.enum).toBe(`variant`)
    expect(coreCatalog.components.Button.props.size.enum).toBe(`size`)
    expect(coreCatalog.components.Badge.props.variant.enum).toBe(`variant`)
  })

  test(`core-lite = the app-shaped vocabulary: no overlays, media, Chart, data tables or the rarer controls`, () => {
    const lite = coreLite()
    expect(lite).toEqual([...LITE_COMPONENTS] as string[])
    // Round 3 (VAPP-102): the app deleted these primitives; they stay in the full catalog only.
    const rarer = [`Pagination`, `Accordion`, `Radio`, `Slider`, `Spinner`, `Table`, `Toggle`, `Carousel`]
    for (const [name, def] of components) {
      if (def.hidden) {
        expect(lite.includes(name), name).toBe(false)
        continue
      }
      const excluded = def.group === `overlay` || def.group === `media` || name === `Chart` || name === `Sparkline` || rarer.includes(name)
      if (name === `Icon` || name === `Avatar`) continue // the two media natives every list needs
      expect(lite.includes(name), name).toBe(!excluded)
    }
    expect(lite).toContain(`Icon`)
    expect(lite).toContain(`Avatar`)
  })

  test(`one sentence per component and per prop, every enum and shape resolves`, () => {
    for (const [name, def] of components) {
      expect(def.description.length, name).toBeGreaterThan(10)
      expect(def.description.endsWith(`.`), `${name} description ends with a full stop`).toBe(true)
      for (const [prop, schema] of Object.entries(def.props)) {
        expect(schema.description.length, `${name}.${prop}`).toBeGreaterThan(5)
        if (schema.type === `enum` && !schema.values) expect(coreCatalog.enums[schema.enum!], `${name}.${prop} enum`).toBeDefined()
        if (schema.type === `object` && schema.shape) expect(coreCatalog.defs[schema.shape], `${name}.${prop} shape`).toBeDefined()
        if (schema.type === `array` && schema.items?.type === `object` && schema.items.shape) expect(coreCatalog.defs[schema.items.shape], `${name}.${prop} items`).toBeDefined()
        // Round 1: a responsive prop is an enum, number or boolean (a value per breakpoint).
        if (schema.responsive) expect([`enum`, `number`, `boolean`], `${name}.${prop} responsive`).toContain(schema.type)
      }
      if (def.slots) for (const slot of def.slots) expect(slot).toMatch(/^([a-z]+|\*)$/)
    }
    for (const [name, def] of Object.entries(coreCatalog.defs)) {
      for (const [prop, schema] of Object.entries(def.properties)) expect(schema.description.length, `${name}.${prop}`).toBeGreaterThan(5)
    }
  })

  test(`every visible component's example validates`, () => {
    for (const [name, def] of components) {
      if (def.hidden) continue
      expect(def.example, `${name} has an example`).toBeDefined()
      expect(validateProps(def, def.example ?? {}), name).toEqual([])
    }
  })

  test(`specimen ids are unique and kebab-cased`, () => {
    const ids = Object.values(SPECIMEN_IDS)
    expect(new Set(ids).size).toBe(ids.length)
    for (const id of ids) expect(id).toMatch(/^exponential-ui-[a-z0-9-]+$/)
  })
})

describe(`tokens and icons`, () => {
  test(`the token groups the issue names, every name unique`, () => {
    expect(Object.keys(TOKEN_GROUPS)).toEqual([`color`, `spacing`, `radius`, `type.size`, `type.lineHeight`, `type.weight`, `type.family`, `control`, `shadow`, `opacity`, `border`, `motion`, `breakpoint`, `ease`, `density`, `blur`])
    expect(TOKEN_GROUPS.breakpoint).toEqual([`sm`, `md`, `lg`, `xl`])
    for (const n of [6, 7, 8]) expect(TOKEN_GROUPS.color).toContain(`chart${n}`)
    for (const [group, names] of Object.entries(TOKEN_GROUPS)) {
      expect(names.length, group).toBeGreaterThan(0)
      expect(new Set(names).size, group).toBe(names.length)
    }
    for (const name of [`background`, `foreground`, `card`, `primary`, `primaryForeground`, `secondary`, `muted`, `mutedForeground`, `accent`, `destructive`, `border`, `input`, `ring`])
      expect(TOKEN_GROUPS.color).toContain(name)
  })

  test(`the icon vocabulary is the icons.json registry`, () => {
    const icons = [...ICON_NAMES] as string[]
    expect(icons.length).toBeGreaterThan(300)
    for (const concept of [`ui-check`, `ui-close`, `nav-inbox`, `ui-icon-placeholder`, `nav-search`]) expect(icons).toContain(concept)
    for (const value of Object.values(basicMap.icons)) if (value) expect(icons, value).toContain(value)
    expect(icons).toContain(basicMap.placeholderIcon)
  })
})

describe(`basic map`, () => {
  test(`every basic component, icon and function is mapped (or the explicit placeholder)`, () => {
    const basic = vendoredBasic as unknown as {
      components: Record<string, { allOf: { properties?: { name?: { oneOf: { enum: string[] }[] } } }[] }>
      functions: Record<string, unknown>
    }
    expect(Object.keys(basicMap.components).sort()).toEqual(Object.keys(basic.components).sort())
    const iconEnum = basic.components.Icon.allOf[2].properties!.name!.oneOf[0].enum
    expect(Object.keys(basicMap.icons).sort()).toEqual([...iconEnum].sort())
    expect(Object.keys(basicMap.functions).sort()).toEqual(Object.keys(basic.functions).sort())
    // Round 1: the basic catalog's 14 functions, then the core ones (functions.core), in that order.
    const core = Object.keys(coreCatalog.functions.core)
    expect([...coreCatalog.functions.names]).toEqual([...Object.keys(basic.functions), ...core])
    expect(core).toEqual([`percent`, `add`, `sub`, `eq`, `lt`, `clamp`, `cond`, `fallback`, `concat`, `coalesce`, `text`, `map`, `len`, `fill`, `set`, `formatPercent`, `formatRelativeTime`])
    for (const rule of Object.values(basicMap.components)) expect(coreCatalog.components[rule.to], rule.to).toBeDefined()
  })
})

describe(`round 1 catalog tables`, () => {
  test(`every built-in icon is an icons.json name and names a real component`, () => {
    const icons = new Set(ICON_NAMES as readonly string[])
    for (const [slot, icon] of Object.entries(coreCatalog.builtinIcons)) {
      if (slot.startsWith(`$`)) continue
      expect(icons.has(icon), `${slot} → ${icon}`).toBe(true)
      expect(coreCatalog.components[slot.split(`.`)[0]], slot).toBeDefined()
    }
  })

  test(`every core function has a spec and the evaluator implements every value function`, () => {
    for (const [name, spec] of Object.entries(coreCatalog.functions.core)) {
      expect(spec.description.endsWith(`.`), name).toBe(true)
      if (name !== `set`) expect(CORE_FUNCTIONS[name], name).toBeDefined()
    }
    expect(Object.keys(CORE_FUNCTIONS).sort()).toEqual(Object.keys(coreCatalog.functions.core).filter((n) => n !== `set`).sort())
  })

  test(`responsive props exist where the contract says (Stack, Grid, Drawer)`, () => {
    expect(coreCatalog.components.Stack.props.direction.responsive).toBe(true)
    expect(coreCatalog.components.Grid.props.columns.responsive).toBe(true)
    expect(coreCatalog.components.Drawer.props.side.responsive).toBe(true)
  })
})
