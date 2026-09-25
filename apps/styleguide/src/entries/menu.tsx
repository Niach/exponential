import { designTokens } from "@exp/design-tokens"
import { MenuSpecimen, conceptIcon, type MenuSpecimenRow } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1074 fills this entry: the menu's SURFACE and ROW recipe, drawn by the
// real class constants on plain elements (`MenuSpecimen`), once per density.

const ROWS: MenuSpecimenRow[] = [
  { kind: `item`, label: `New issue`, icon: conceptIcon(`ui-add`) },
  { kind: `submenu`, label: `Move to board`, icon: conceptIcon(`nav-boards`), value: `App` },
  { kind: `submenu`, label: `Labels`, icon: conceptIcon(`settings-labels`), value: `None` },
  { kind: `separator` },
  { kind: `item`, label: `Delete issue`, icon: conceptIcon(`ui-delete`), destructive: true },
]

const { pointer, touch } = designTokens.menu
const geometry = (density: typeof pointer) =>
  `${density.itemHeight} row · ${density.itemPaddingX} pad · ${density.itemGap} gap · ${density.iconSize} glyph · ${density.minWidth}–${density.maxWidth} wide`

export const entry: StyleguideEntry = {
  id: `menu`,
  section: `general`,
  owner: `EXP-1074`,
  title: `Menu`,
  blurb: `The one floating-menu surface (opaque card fill, hairline, radius 12, no blur, no shadow) and the one ROW recipe on it, in two densities from tokens.json \`menu\`: POINTER for web from md up (${geometry(pointer)}), TOUCH for mobile web, iOS and Android (${geometry(touch)}). Every menu-like row on web reads the same --menu-* vars — context and dropdown items, their sub-triggers, Select, Command, the typeahead — so one change moves every menu. A destructive row is red and never fenced off by a divider (EXP-687).`,
  status: {
    web: {
      state: `ok`,
      symbol: `MENU_ITEM_CLASS`,
      file: `packages/ui/src/menu-surface.ts`,
      note: `--menu-* in styles.css mirror tokens.json; this specimen wears the same constants`,
    },
    desktop: {
      state: `leftover`,
      symbol: `menu::pointer`,
      file: `apps/desktop/crates/theme/src/tokens.generated.rs`,
      note: `gpui-component's PopupMenu draws 26px rows / 8px pad from the crate; the tokens record 36 / 8`,
    },
    ios: {
      state: `ok`,
      symbol: `GlassMenuTokens`,
      file: `apps/ios/ExpUI/Sources/GlassMenu.swift`,
      note: `the touch set: 48pt rows, 12pt padding`,
    },
    android: {
      state: `ok`,
      symbol: `GlassMenuDefaults`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/components/GlassMenu.kt`,
      note: `the touch set: M3's 48dp rows, 12dp padding`,
    },
  },
  island: () => (
    <div className="flex flex-wrap items-start gap-6">
      <div className="grid gap-2">
        <MenuSpecimen rows={ROWS} density="pointer" />
        <p className="text-xs text-muted-foreground">pointer · web from md up</p>
      </div>
      <div className="grid gap-2">
        <MenuSpecimen rows={ROWS} density="touch" />
        <p className="text-xs text-muted-foreground">touch · phones, iOS, Android</p>
      </div>
    </div>
  ),
}
