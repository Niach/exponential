import composerMenu from "@exp/domain-contract/fixtures/composer-menu.json"
import type { IconConcept } from "@exp/icons"
import {
  MENU_SURFACE_CLASS,
  MenuPanel,
  SearchList,
  actionPickerItems,
  conceptIcon,
  type MenuEntry,
} from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// SLOP-28 (EXP-1259 D5, `composer-menu.json`): a `Menu` submenu whose body is
// THE search list. On a pointer it opens to the SIDE of its row (hover or →;
// ← with the caret at the field's start, or Escape, closes back to the
// parent row); on a phone it pushes as a page of the same sheet. The
// specimens draw the real `MenuPanel` rows at rest beside the submenu's
// surface (the live one is a Radix portal).

const noop = (): void => {}
const BackGlyph = conceptIcon(`ui-chevron-left`)

const AGENTS = [
  { id: `g1`, name: `Release manager`, icon: `rocket` },
  { id: `g2`, name: `Docs writer`, icon: `megaphone` },
  { id: `g3`, name: `Triage`, icon: `wrench` },
  { id: `g4`, name: `Merge conflicts`, icon: `git-branch` },
]

/** The "+" menu's first group, straight from the fixture. */
function plusEntries(): MenuEntry[] {
  return composerMenu.rows.slice(0, 3).map((row): MenuEntry => {
    const base = {
      id: row.id,
      label: row.label ?? ``,
      icon: conceptIcon(row.icon as IconConcept),
    }
    return row.kind === `submenu`
      ? { ...base, kind: `submenu`, entries: [] }
      : { ...base, kind: `item`, onSelect: noop }
  })
}

function AgentList({ density }: { density: `pointer` | `phone` }) {
  return (
    <SearchList
      mode="single"
      density={density}
      items={actionPickerItems(AGENTS)}
      value={null}
      onChange={noop}
      placeholder="Search agents"
    />
  )
}

export const entry: StyleguideEntry = {
  id: `menu-submenu`,
  section: `general`,
  owner: `SLOP-28`,
  title: `Menu submenu`,
  blurb: `A \`Menu\` row with a › opens its submenu: on a pointer to the side of the row, on hover or →, and ← (the caret at the start of its search field) or Escape closes it back to the parent row; on a phone it pushes as a page of the same sheet, a back row on top. The composer's Implement issue › and Message an agent › bodies are THE search list (Search list above): ONE row height (${composerMenu.submenuRowHeight}px), at most ${composerMenu.submenuMaxRows} rows visible, an agent row = its icon and its NAME (no description).`,
  status: {
    web: {
      state: `ok`,
      symbol: `Menu (MenuSubmenuEntry.body)`,
      file: `packages/ui/src/menu.tsx`,
      note: `the composer's bodies = apps/web launch-dialog/composer-plus-menu.tsx`,
    },
    desktop: {
      state: `ok`,
      symbol: `PointerMenu`,
      file: `apps/desktop/crates/ui/src/controls.rs`,
    },
    ios: {
      state: `ok`,
      symbol: `GlassMenu`,
      file: `apps/ios/ExpUI/Sources/GlassMenu.swift`,
      note: `the composer's sheet = ComposerPlusMenu.swift, submenus as pushed pages`,
    },
    android: {
      state: `ok`,
      symbol: `GlassMenu`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/components/GlassMenu.kt`,
      note: `the composer's sheet = ui/agent/ComposerPlusMenu.kt`,
    },
  },
  island: () => (
    <div className="flex flex-wrap items-start gap-8">
      <div className="flex items-start gap-1">
        <MenuPanel entries={plusEntries()} density="pointer" />
        <div
          className={`${MENU_SURFACE_CLASS} flex w-[26rem] flex-col overflow-hidden p-0`}
          style={{ marginTop: 40 }}
        >
          <AgentList density="pointer" />
        </div>
      </div>
      <div className="flex w-[24rem] flex-col rounded-t-3xl border-t border-glass-stroke-card bg-glass-bottom pt-2 pb-4">
        <div aria-hidden className="mx-auto mb-2 h-1 w-9 rounded-full bg-foreground/20" />
        <div className="flex items-center gap-1 px-3 pb-1 text-base font-semibold">
          <BackGlyph aria-hidden className="size-5 text-muted-foreground" />
          Message an agent
        </div>
        <AgentList density="phone" />
      </div>
    </div>
  ),
}
