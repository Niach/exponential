import { BoardPicker, PickerList, PickerTrigger, boardPickerItems } from "@exp/ui"

import { PickerSpecimen, typedPickerStatus } from "./picker-shared.tsx"
import type { StyleguideEntry } from "./types.ts"

const noop = (): void => {}

/* A repo-backed board with no icon of its own draws the repo glyph, and the
   colour tints the glyph rather than sitting beside it as an anonymous dot
   (EXP-449) — that is what `boardPickerItems` decides, once, for all four. */
const BOARDS = [
  { id: `mobile`, name: `Mobile app`, icon: `smartphone`, color: `#3b82f6` },
  { id: `web`, name: `Website`, icon: `globe`, color: `#22c55e` },
  { id: `api`, name: `API`, color: `#a855f7`, repositoryId: `repo-1` },
  { id: `ops`, name: `Operations`, color: `#f59e0b` },
]

export const entry: StyleguideEntry = {
  id: `picker-board`,
  section: `general`,
  owner: `EXP-1021`,
  title: `Board picker`,
  blurb: `Boards by icon and colour, everywhere a board is picked: the create-issue dialog's prefix chip, move-to-board (the issue detail, the list row's context menu, the phone properties sheet) and the automation trigger's board filter, which is the same picker in its multi arm. A board with no icon of its own is never a bare colour dot — a repo-backed board draws the repo glyph, everything else the default board glyph (EXP-449), decided once in \`boardPickerItems\`.`,
  status: typedPickerStatus(`BoardPicker`, {
    web: `board-picker.tsx`,
    desktop: `board_picker.rs`,
    ios: `BoardPicker.swift`,
    android: `BoardPicker.kt`,
  }),
  island: () => (
    <PickerSpecimen
      trigger={
        <BoardPicker
          boards={BOARDS}
          value="mobile"
          onChange={noop}
          trigger={
            <PickerTrigger variant="pill" label="Board" value="Mobile app" />
          }
        />
      }
      surface={
        <PickerList
          mode="single"
          search
          searchPlaceholder="Search boards…"
          items={boardPickerItems(BOARDS)}
          value="mobile"
          onChange={noop}
        />
      }
      caption="The multi arm is the same rows, capped, for an automation's board filter."
    />
  ),
}
