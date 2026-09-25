import { MenuSpecimen, issueMenuSpecimenRows } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1074 fills this entry: THE issue context menu, at rest, from the one
// layout the web draws live and the IDE mirrors (`ISSUE_MENU_LAYOUT`).

export const entry: StyleguideEntry = {
  id: `issue-context-menu`,
  section: `special`,
  owner: `EXP-1074`,
  title: `Issue context menu`,
  blurb: `The menu a right-click (or a phone's long-press) opens on ANY issue: a board row, the sidebar list, a reviews row, an issue chip. One host per team layout on web — an element opts in with its issue id, the menu resolves the issue live — and one layout: the header band, Open / Mark as done / Copy issue ID (Select on a phone, Unmark duplicate on a duplicate), a divider, then the field submenus — Status, Assignee, Priority, Labels, Estimate (teams that estimate), Set due date, Move to board (teams with several), Add relation — and Delete issue in red, with no divider above it. Each submenu shows the current value beside its label; the label never wraps, the value truncates.`,
  status: {
    web: {
      state: `ok`,
      symbol: `ISSUE_MENU_LAYOUT`,
      file: `packages/ui/src/issue-menu.ts`,
      note: `drawn live by apps/web/src/components/issue-context-menu (its test locks the order to this layout)`,
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
    <MenuSpecimen
      rows={issueMenuSpecimenRows(new Set([`estimation`, `boards`]), {
        identifier: `EXP-1074`,
        title: `web: context menu misbehaving`,
      })}
    />
  ),
}
