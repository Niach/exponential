import { MenuPanel, issueMenuSampleEntries } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1074 + the UI cleanup batch: THE issue context menu, rendered by the
// real `Menu` renderer at rest (`MenuPanel`) from the ONE layout the web
// draws live and the IDE mirrors (`ISSUE_MENU_LAYOUT` → `issueMenuEntries`).

const HEADER = { identifier: `EXP-1074`, title: `web: context menu misbehaving` }

export const entry: StyleguideEntry = {
  id: `issue-context-menu`,
  section: `special`,
  owner: `EXP-1074`,
  title: `Issue context menu`,
  blurb: `The menu a right-click (or a phone's long-press) opens on ANY issue: a board row, the sidebar list, a reviews row, an issue chip. One gesture host per team layout on web — an element opts in with its issue id, the menu resolves the issue live — and one layout the live menu RENDERS (issueMenuEntries): the header band, Open / Mark as done / Copy issue ID (Select on a phone, Unmark duplicate on a duplicate), a divider, then the field submenus — Status, Assignee, Priority, Labels, Estimate (teams that estimate), Set due date, Move to board (teams with several), Add relation — and Delete issue in red, with no divider above it. Each submenu shows the current value beside its label and opens the typed picker's own rows. On a phone the same rows are a bottom sheet, the submenus pushed pages.`,
  status: {
    web: {
      state: `ok`,
      symbol: `ISSUE_MENU_LAYOUT / issueMenuEntries`,
      file: `packages/ui/src/issue-menu.ts`,
      note: `rendered live by apps/web/src/components/issue-context-menu through Menu (pointer; a sheet below md)`,
    },
    desktop: {
      state: `ok`,
      symbol: `build_row_context_menu`,
      file: `apps/desktop/crates/ui/src/issue_list.rs`,
      note: `the same rows in the same order, Estimate included (EXP-1077); Delete confirms in a dialog`,
    },
    ios: {
      state: `n/a`,
      note: `no row menu by decision (EXP-698 r5): status and priority sit on the row, a selection does the rest`,
    },
    android: {
      state: `n/a`,
      note: `a long-press selects (EXP-239); the My issues long-press sheet keeps Mark done / Move to backlog`,
    },
  },
  island: () => (
    <div className="flex flex-wrap items-start gap-8">
      <MenuPanel
        entries={issueMenuSampleEntries(new Set([`estimation`, `boards`]), HEADER)}
        density="pointer"
      />
      <div className="w-[24rem]">
        <MenuPanel
          entries={issueMenuSampleEntries(new Set([`phone`, `estimation`, `boards`]), HEADER)}
          look="sheet"
          density="touch"
        />
      </div>
    </div>
  ),
}
