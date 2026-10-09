import { describe, expect, it, vi } from "vitest"
import { conceptIcon } from "./icons.generated"
import { menuEntriesFromCatalog, type MenuItemLike } from "./menu"

// VAPP-102: the catalog's `menuItem` rows feed the app's ONE menu.
describe(`menuEntriesFromCatalog`, () => {
  const items: MenuItemLike[] = [
    { kind: `label`, label: `Issue` },
    { label: `Copy link`, value: `copy`, icon: `ui-copy`, shortcut: `⌘C` },
    { kind: `checkbox`, label: `Subscribed`, value: `sub`, checked: true },
    { kind: `separator` },
    {
      kind: `submenu`,
      label: `Status`,
      value: `status`,
      items: [
        { label: `Backlog`, value: `backlog` },
        { kind: `submenu`, label: `Too deep`, items: [{ label: `x` }] },
      ],
    },
    { label: `Delete`, value: `delete`, destructive: true, disabled: true },
  ]

  it(`maps every kind`, () => {
    const entries = menuEntriesFromCatalog(items, vi.fn())
    expect(entries.map((e) => e.kind)).toEqual([`header`, `item`, `toggle`, `separator`, `submenu`, `item`])
    expect(entries[0]).toMatchObject({ kind: `header`, title: `Issue` })
    expect(entries[1]).toMatchObject({ kind: `item`, id: `copy`, label: `Copy link`, shortcut: `⌘C`, icon: conceptIcon(`ui-copy`) })
    expect(entries[2]).toMatchObject({ kind: `toggle`, checked: true })
    expect(entries[5]).toMatchObject({ kind: `item`, destructive: true, disabled: true })
  })

  it(`keeps one level of submenu`, () => {
    const sub = menuEntriesFromCatalog(items, vi.fn())[4]!
    expect(sub.kind).toBe(`submenu`)
    if (sub.kind !== `submenu`) return
    expect(sub.entries!.map((e) => e.kind)).toEqual([`item`])
  })

  it(`reports the value, the row and a checkbox's new state`, () => {
    const onSelect = vi.fn()
    const entries = menuEntriesFromCatalog(items, onSelect)
    const copy = entries[1]!
    if (copy.kind === `item`) copy.onSelect()
    expect(onSelect).toHaveBeenLastCalledWith(`copy`, items[1])
    const sub = entries[2]!
    if (sub.kind === `toggle`) sub.onChange(false)
    expect(onSelect).toHaveBeenLastCalledWith(`sub`, items[2], false)
    const status = entries[4]!
    if (status.kind === `submenu` && status.entries?.[0]?.kind === `item`) status.entries[0].onSelect()
    expect(onSelect).toHaveBeenLastCalledWith(`backlog`, { label: `Backlog`, value: `backlog` })
  })

  it(`reads an unresolved binding as unchecked / empty`, () => {
    const entries = menuEntriesFromCatalog(
      [
        { kind: `checkbox`, label: `A`, checked: { path: `/a` } },
        { kind: `submenu`, label: `B`, items: { path: `/rows` } },
      ],
      vi.fn()
    )
    expect(entries[0]).toMatchObject({ kind: `toggle`, checked: false })
    expect(entries[1]).toMatchObject({ kind: `submenu`, entries: [] })
  })

  it(`falls back to the label as the value`, () => {
    const onSelect = vi.fn()
    const [entry] = menuEntriesFromCatalog([{ label: `Open` }], onSelect)
    if (entry?.kind === `item`) entry.onSelect()
    expect(onSelect).toHaveBeenCalledWith(`Open`, { label: `Open` })
  })
})
