import { designTokens } from "@exp/design-tokens"
import composerMenu from "@exp/domain-contract/fixtures/composer-menu.json"
import type { IconConcept } from "@exp/icons"
import {
  MenuPanel,
  PickerMenuRows,
  conceptIcon,
  issueMenuSampleEntries,
  type MenuEntry,
} from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// UI cleanup batch — THE menu, rendered by the REAL renderer: `MenuPanel` is
// `Menu`'s own row renderer as a static surface (a closed Radix portal draws
// nothing to static markup), fed the same `MenuEntry[]` the app hands `Menu`.
// Three presentations, three specimens: an action's `…` dropdown, the issue
// context menu at the pointer, and the composer's "+" menu as the phone sheet.

const noop = (): void => {}

/** The action row's `…` (team-actions-panel.tsx `ActionMenu`). */
const ACTION_MENU: MenuEntry[] = [
  { kind: `item`, label: `Pin`, icon: conceptIcon(`ui-pin`), onSelect: noop },
  { kind: `item`, label: `Edit`, icon: conceptIcon(`ui-edit`), onSelect: noop },
  { kind: `item`, label: `Delete`, icon: conceptIcon(`ui-delete`), destructive: true, onSelect: noop },
]

/** The composer's "+" (EXP-1249), straight from `composer-menu.json` (the
 *  fixture the four clients replay), every condition met; Effort opens its
 *  picker body to show the PickerMenuRows language. */
const SAMPLE_VALUES: Record<string, string> = {
  effort: `High`,
  subagents: `Opus`,
  "mcp-servers": `2`,
}
const SAMPLE_CHECKED: Record<string, boolean> = { "computer-use": true }

const COMPOSER_MENU: MenuEntry[] = composerMenu.rows.map((row): MenuEntry => {
  if (row.kind === `separator`) return { kind: `separator` }
  const id = row.id ?? ``
  const base = { id, label: row.label ?? ``, icon: conceptIcon(row.icon as IconConcept) }
  if (row.kind === `toggle`) {
    return { ...base, kind: `toggle`, checked: SAMPLE_CHECKED[id] ?? false, onChange: noop }
  }
  if (row.kind === `submenu` && id === `effort`) {
    return {
      ...base,
      kind: `submenu`,
      value: SAMPLE_VALUES[id],
      body: (
        <PickerMenuRows
          mode="single"
          items={[
            { value: `low`, label: `Low` },
            { value: `high`, label: `High` },
          ]}
          value="high"
          onChange={noop}
        />
      ),
    }
  }
  if (row.kind === `submenu`) {
    return { ...base, kind: `submenu`, value: SAMPLE_VALUES[id], entries: [] }
  }
  return { ...base, kind: `item`, onSelect: noop }
})

const ISSUE_MENU = issueMenuSampleEntries(new Set([`estimation`, `boards`]), {
  identifier: `EXP-1239`,
  title: `ios double button`,
})

const { pointer, touch } = designTokens.menu
const geometry = (density: typeof pointer) =>
  `${density.itemHeight} row · ${density.itemPaddingX} pad · ${density.itemGap} gap · ${density.iconSize} glyph`

function Caption({ children }: { children: string }) {
  return <p className="text-xs text-muted-foreground">{children}</p>
}

export const entry: StyleguideEntry = {
  id: `menu`,
  section: `general`,
  owner: `EXP-1074`,
  title: `Menu`,
  blurb: `ONE data-driven menu for every action menu: a list of MenuEntry rows (item, submenu with more rows or a picker body, toggle, separator, header) handed to one renderer with three presentations — a DROPDOWN at a trigger, a CONTEXT menu at the pointer (one document gesture host per layout; an issue row, a work tab or an action opts in with menuProps(kind, id)), and a bottom SHEET of the same rows at touch density below md, submenus as pushed pages. Every row is the one recipe: the --menu-* geometry from tokens.json (pointer from md up: ${geometry(pointer)}; touch: ${geometry(touch)}), the POINTER cursor on hover, a destructive row in red with no divider above it (EXP-687). A submenu's picker rows are PickerMenuRows — the picker's own selection language (single = check, multi = highlight). These specimens are the real renderer at rest (MenuPanel), fed the app's entries.`,
  status: {
    web: {
      state: `ok`,
      symbol: `Menu / MenuGestureHost / MenuPanel`,
      file: `packages/ui/src/menu.tsx`,
      note: `rows wear MENU_ITEM_CLASS (packages/exponential-ui-react/src/primitives/menu-surface.ts)`,
    },
    desktop: {
      state: `leftover`,
      symbol: `PopupMenu + controls::PointerMenu`,
      file: `apps/desktop/crates/ui/src/styleguide/entries/menu.rs`,
      note: `pointer rows (controls::pointer_menu_item) are 32px with the pointer cursor against the 36px token; the pickers' option rows are still the crate's 26px`,
    },
    ios: {
      state: `ok`,
      symbol: `GlassMenu`,
      file: `apps/ios/ExpUI/Sources/GlassMenu.swift`,
      note: `the touch set: 48pt rows, 12pt padding`,
    },
    android: {
      state: `ok`,
      symbol: `GlassDropdownMenu`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/components/GlassMenu.kt`,
      note: `the touch set: M3's 48dp rows, 12dp padding`,
    },
  },
  island: () => (
    <div className="flex flex-wrap items-start gap-8">
      <div className="grid gap-2">
        <MenuPanel entries={ACTION_MENU} density="pointer" />
        <Caption>dropdown · an action's … (trigger)</Caption>
      </div>
      <div className="grid gap-2">
        <MenuPanel entries={ISSUE_MENU} density="pointer" />
        <Caption>context · the issue menu at the pointer</Caption>
      </div>
      <div className="grid w-[24rem] gap-2">
        <MenuPanel entries={COMPOSER_MENU} look="sheet" density="touch" />
        <Caption>sheet · the composer "+" on a phone (same rows, touch density)</Caption>
      </div>
      <div className="grid gap-2">
        <MenuPanel entries={COMPOSER_MENU} density="pointer" />
        <Caption>the same "+" as a dropdown from md up</Caption>
      </div>
    </div>
  ),
}
