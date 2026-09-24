import {
  PickerList,
  PickerTrigger,
  conceptIcon,
  type PickerItem,
} from "@exp/ui"

import type { ComponentPlatform, ComponentStatus } from "../components.tsx"
import { PICKER_FILES, PickerSpecimen } from "./picker-shared.tsx"
import type { StyleguideEntry } from "./types.ts"

// EXP-1029 contract, EXP-1021 implementation — THE picker primitive.

const noop = (): void => {}
const BoardGlyph = conceptIcon(`nav-boards`)

/* The resting state of a picker is its TRIGGER; its surface is a portal that
   renders nothing to static markup, so the specimen shows the surface's BODY
   (`PickerList`) beside it — the same rows, the same selection language, one
   level down from the popover. */
const ROWS: PickerItem[] = [
  { value: `mobile`, label: `Mobile app`, icon: BoardGlyph, color: `#3b82f6` },
  { value: `web`, label: `Website`, icon: BoardGlyph, color: `#22c55e` },
  {
    value: `infra`,
    label: `Infrastructure`,
    icon: BoardGlyph,
    color: `#a855f7`,
    description: `9 open issues`,
  },
  { value: `archive`, label: `Archive`, icon: BoardGlyph, disabled: true },
]

const status: Record<ComponentPlatform, ComponentStatus> = {
  web: { state: `ok`, symbol: `Picker`, file: `${PICKER_FILES.web}/picker.tsx` },
  desktop: {
    state: `ok`,
    symbol: `picker::Picker`,
    file: `${PICKER_FILES.desktop}/mod.rs`,
  },
  ios: {
    state: `ok`,
    symbol: `GlassPicker`,
    file: `${PICKER_FILES.ios}/Picker.swift`,
    note: `SwiftUI owns the bare name Picker, so the primitive is GlassPicker; the ten typed ones keep theirs.`,
  },
  android: {
    state: `ok`,
    symbol: `Picker`,
    file: `${PICKER_FILES.android}/Picker.kt`,
  },
}

export const entry: StyleguideEntry = {
  id: `picker`,
  section: `general`,
  owner: `EXP-1021`,
  title: `Picker`,
  blurb: `THE picker primitive: one surface per platform (a popover at the trigger on a pointer, a bottom sheet of plain rows on a phone), single or multi, optional search. Presentation belongs to the primitive, never to the caller — a row is a glyph or a dot, a label and a muted second line, and it is never a card. The selection language is the whole point of EXP-1021: a single pick wears a trailing check, a MULTI pick reads as the row's own highlight, never a leading circle, so the picker that links a relation and the picker that batches issues finally look like one thing. The trigger is the caller's (four shapes: pill, field, row, inline); the surface, the search field and the keys are the primitive's.`,
  status,
  island: () => (
    <div className="grid gap-5">
      <div className="flex flex-wrap items-center gap-3">
        <PickerTrigger variant="pill" label="Board" value="Mobile app" />
        <PickerTrigger variant="field" label="Board" value="Mobile app" />
        <PickerTrigger variant="inline" label="Board" value="Mobile app" />
      </div>
      <div className="w-[18rem] overflow-hidden rounded-lg border border-glass-stroke bg-popover">
        <PickerTrigger variant="row" label="Board" value="Mobile app" />
      </div>
      <PickerSpecimen
        trigger={
          <PickerTrigger variant="pill" label="Board" value="Website" />
        }
        surface={
          <PickerList
            mode="single"
            search
            searchPlaceholder="Search boards…"
            items={ROWS}
            value="web"
            onChange={noop}
          />
        }
        caption="Single: one pick, a trailing check, the surface closes."
      />
      <PickerSpecimen
        trigger={
          <PickerTrigger variant="field" label="Boards" value="Website +1" />
        }
        surface={
          <PickerList
            mode="multi"
            items={ROWS}
            value={[`web`, `infra`]}
            onChange={noop}
          />
        }
        caption="Multi: the picked rows are the highlighted ones — no circles, nothing in the gutter."
      />
    </div>
  ),
}
