// VAPP-85: the Exponential app's extension catalog for Exponential UI is
// valid against the SDK, expands through the core macro table, and names
// only components the app already draws (docs/shared-ui-inventory.md).
// VAPP-102: the list rows sit on the catalog's Row/Section/Chip vocabulary.

import { describe, expect, it } from "vitest"
import {
  catalogPrompt,
  coreCatalog,
  defineExtension,
  expandMacros,
  reduceSurface,
  type ExtensionDef,
  type FlatComponent,
  type UiNode,
} from "@exponential-at/ui"
import extensionJson from "../exponential-ui/extension.json"

const extension = defineExtension(extensionJson as unknown as ExtensionDef)

/** The root node first; `more` = the flat components its `children` name. */
const reduce = (node: Record<string, unknown>, more: Record<string, unknown>[] = []) =>
  reduceSurface([{ id: `root`, ...node } as FlatComponent, ...(more as FlatComponent[])], {
    catalogId: extension.id,
    extensions: [extension],
  })

/** Every node of a tree, slots included. */
const walk = (node: UiNode): UiNode[] => [
  node,
  ...Object.values(node.slots ?? {}).flatMap(walk),
  ...node.children.flatMap(walk),
]

/** Expanded = ids unique, every node a native (core or this extension's). */
function expectNative(root: UiNode) {
  const nodes = walk(root)
  const ids = nodes.map((n) => n.id)
  expect(new Set(ids).size, ids.join(`,`)).toBe(ids.length)
  for (const n of nodes) {
    const def = coreCatalog.components[n.component] ?? extension.components[n.component]
    expect(def?.kind, n.component).toBe(`native`)
  }
}

describe(`Exponential app extension (VAPP-85)`, () => {
  it(`is a valid extension of the core catalog`, () => {
    expect(extension.id).toBe(`https://ui.exponential.at/catalogs/exponential-app/v1`)
    expect(Object.keys(extension.components)).toEqual([
      `IssueRow`,
      `IssueGroupBand`,
      `RunStatusRow`,
      `SessionRow`,
      `PrRow`,
      `StackRail`,
      `IssueChip`,
      `LabelChip`,
      `StatusGlyph`,
      `LiveDot`,
      `IconDisc`,
      `ComposerHeadline`,
      `DecisionCard`,
      `ChoiceDialog`,
      `ResultTile`,
      `ResultGroup`,
      `Diff`,
      `DiffCounts`,
      `DiffFileRow`,
      `GuideSection`,
    ])
    expect(Object.keys(extension.macros ?? {}).sort()).toEqual(
      Object.entries(extension.components).filter(([, d]) => d.kind === `macro`).map(([n]) => n).sort()
    )
  })

  it(`every example reduces without issues; the macros expand to core natives`, () => {
    for (const [name, def] of Object.entries(extension.components)) {
      const { root, issues } = reduce({ component: name, ...(def.example ?? {}) })
      expect(issues, name).toEqual([])
      if (def.kind === `macro`) {
        expect(root.component, name).not.toBe(name)
        expectNative(root)
      } else expect(root.component).toBe(name)
    }
    const chip = reduce({ component: `LabelChip`, name: `bug`, color: `#EF4444` }).root
    expect(chip.component).toBe(`Box`)
    expect(chip.recipe).toEqual({ macro: `Chip`, part: `root`, props: { shape: `pill`, selected: false, tone: `neutral` } })
    expect(chip.children.map((c) => c.id)).toEqual([`root.dot`, `root.label`])
    expect(expandMacros(chip)).toEqual(chip)
  })

  it(`IssueRow is a pressable Row: the StatusGlyph leads, labels + avatar trail`, () => {
    const { root, issues } = reduce({
      component: `IssueRow`,
      identifier: `EXP-12`,
      title: `Rotate the signing key`,
      status: { icon: `status-in-progress` },
      priority: `high`,
      labels: [
        { label: `bug`, value: `#EF4444` },
        { label: `ui`, value: `#3B82F6` },
      ],
      assigneeName: `Ada`,
      on: { press: { event: { name: `open` } } },
    })
    expect(issues).toEqual([])
    expectNative(root)
    expect(root.recipe).toMatchObject({ macro: `Row`, part: `root` })
    expect(root.props.pressable).toBe(true)
    expect(root.on?.press).toEqual({ event: { name: `open` } })
    const parts = root.children.map((c) => c.recipe?.part ?? c.component)
    expect(parts[0]).toBe(`status`)
    expect(root.children[0]!.component).toBe(`StatusGlyph`)
    expect(root.children[0]!.props.status).toEqual({ icon: `status-in-progress` })
    expect(parts).toContain(`identifier`)
    const nodes = walk(root)
    const labelRoots = nodes.filter((n) => n.recipe?.macro === `Chip` && n.recipe.part === `root`)
    expect(labelRoots).toHaveLength(2)
    expect(nodes.some((n) => n.component === `Avatar` && n.props.name === `Ada`)).toBe(true)
    expect(nodes.some((n) => n.component === `Icon` && n.props.name === `priority-high`)).toBe(true)
  })

  it(`IssueRow depth draws the core's tree guides`, () => {
    const { root } = reduce({
      component: `Section`,
      tree: true,
      children: [`a`, `b`],
    }, [
      { id: `a`, component: `IssueRow`, identifier: `EXP-1`, title: `Parent`, status: { icon: `status-backlog` } },
      { id: `b`, component: `IssueRow`, identifier: `EXP-2`, title: `Child`, status: { icon: `status-backlog` }, depth: 1 },
    ])
    const child = walk(root).find((n) => n.id === `b`)!
    const guides = child.children.find((c) => c.component === `TreeGuides`)!
    expect(guides.props).toEqual({ depth: 1, elbowAt: 0, tee: false, passThrough: [] })
  })

  it(`IssueChip is a rect Chip led by the StatusGlyph, press + remove routed`, () => {
    const { root, issues } = reduce({
      component: `IssueChip`,
      identifier: `EXP-874`,
      title: `Batch runs`,
      status: { icon: `status-done` },
      removable: true,
      on: { press: { event: { name: `open` } }, remove: { event: { name: `drop` } } },
    })
    expect(issues).toEqual([])
    expectNative(root)
    expect(root.recipe).toMatchObject({ macro: `Chip`, part: `root`, props: { shape: `rect` } })
    expect(root.children[0]!.component).toBe(`StatusGlyph`)
    expect(root.on?.press).toEqual({ event: { name: `open` } })
    const remove = root.children.find((c) => c.recipe?.part === `remove`)!
    expect(remove.on?.press).toEqual({ event: { name: `drop` } })
  })

  it(`IssueGroupBand is a Section headed by the StatusGlyph over its rows`, () => {
    const { root, issues } = reduce({
      component: `IssueGroupBand`,
      title: `In Progress`,
      status: { icon: `status-in-progress` },
      count: 2,
      children: [`r1`],
    }, [{ id: `r1`, component: `IssueRow`, identifier: `EXP-1`, title: `One`, status: { icon: `status-in-progress` } }])
    expect(issues).toEqual([])
    expectNative(root)
    expect(root.recipe).toMatchObject({ macro: `Section`, part: `root` })
    const header = root.children.find((c) => c.recipe?.part === `header`)!
    expect(header.children.find((c) => c.component === `StatusGlyph`)).toBeTruthy()
    expect(header.children.find((c) => c.recipe?.part === `count`)?.props.text).toBe(`2`)
    const body = root.children.find((c) => c.recipe?.part === `body`)!
    expect(body.children.map((c) => c.id)).toEqual([`r1`])
  })

  it(`GuideSection = its text, the author's rows, then ONE Changes row`, () => {
    const { root, issues } = reduce({
      component: `GuideSection`,
      title: `Reducer`,
      text: `Moves the merge.`,
      files: 3,
      additions: 40,
      deletions: 12,
      children: [`f1`],
      on: { changes: { event: { name: `openChanges` } } },
    }, [{ id: `f1`, component: `DiffFileRow`, path: `src/a.ts` }])
    expect(issues).toEqual([])
    expectNative(root)
    const body = root.children.find((c) => c.recipe?.part === `body`)!
    expect(body.children.map((c) => c.component)).toEqual([`Markdown`, `DiffFileRow`, `Box`])
    const changes = body.children[2]!
    expect(changes.recipe).toMatchObject({ macro: `Row`, part: `root` })
    expect(changes.on?.press).toEqual({ event: { name: `openChanges` } })
    const texts = walk(changes).filter((n) => n.component === `Text`).map((n) => n.props.text)
    expect(texts).toEqual([`Changes`, `3 files · +40 −12`])
  })

  it(`is listed in the catalog prompt after the core`, () => {
    const prompt = catalogPrompt({ extensions: [extension] })
    expect(prompt).toContain(`Extension ${extension.id} (Exponential app):`)
    expect(prompt).not.toContain(`\nTreeGuides:`)
    expect(prompt.indexOf(`\nIssueRow:`)).toBeGreaterThan(prompt.indexOf(`\nRow:`))
  })
})
