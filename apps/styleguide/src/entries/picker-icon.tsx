import {
  BOARD_ICON_OPTIONS,
  DEVICE_ICON_OPTIONS,
  IconPicker,
  IconSwatchGrid,
} from "@exp/ui"

import { typedPickerStatus } from "./picker-shared.tsx"
import type { StyleguideEntry } from "./types.ts"

const noop = (): void => {}

export const entry: StyleguideEntry = {
  id: `picker-icon`,
  section: `general`,
  owner: `EXP-1021`,
  title: `Icon picker`,
  blurb: `EXP-575/EXP-924: the one icon picker, and the SET is a parameter — the curated board icons unless a surface names another (the device set). It is the primitive with a \`panel\`: the swatch grid REPLACES the search field and the rows, so the surface, its sheet and its trigger stay the picker's while the body is a grid of squares. The trigger is that shape rule: an icon-only ACTION is a circle, an icon PICKER is a rounded square, because it previews a swatch. Both sets are APPEND-ONLY (contract \`boardIcon\` / \`deviceIcon\`): reordering orphans rows.`,
  status: typedPickerStatus(`IconPicker`, {
    web: `icon-picker.tsx`,
    desktop: `icon_picker.rs`,
    ios: `SharedIconPicker.swift`,
    android: `IconPicker.kt`,
  }),
  island: () => (
    <div className="grid gap-4">
      <div className="flex items-center gap-2">
        {/* Picked, picked-with-a-colour, and empty (the dashed square). */}
        <IconPicker value="rocket" onChange={noop} />
        <IconPicker value="flag" onChange={noop} color="#3b82f6" />
        <IconPicker value="" onChange={noop} />
        <IconPicker
          value={DEVICE_ICON_OPTIONS[0]!.name}
          onChange={noop}
          options={DEVICE_ICON_OPTIONS}
          mobileTitle="Device icon"
        />
      </div>
      {/* The surface's body: a closed portal renders nothing, so the grid is
          shown at the size the panel gives it. */}
      <div className="w-max overflow-hidden rounded-lg border border-glass-stroke bg-popover p-3">
        <IconSwatchGrid
          value="rocket"
          onChange={noop}
          options={BOARD_ICON_OPTIONS.slice(0, 24)}
          color="#3b82f6"
        />
      </div>
    </div>
  ),
}
