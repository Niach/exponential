import { CheckCheck, Copy, SquareCheckBig, SquarePen, Undo2 } from "lucide-react"

import { conceptIcon } from "./icons.generated"
import type {
  MenuEntry,
  MenuIcon,
  MenuItemEntry,
  MenuSubmenuEntry,
} from "./menu"

// EXP-1074 — THE issue context menu's layout, in one place; since the UI
// cleanup batch it is DATA the live menu renders, not a list a test compares
// hand-written JSX against.
//
// The web turns it into `MenuEntry[]` (`issueMenuEntries`, consumed by
// apps/web/src/components/issue-context-menu/session.tsx through `Menu`),
// the IDE mirrors it (crates/ui/src/issue_list.rs `build_row_context_menu`),
// and the styleguide renders the SAME entries at rest (`MenuPanel`). `when`
// names the rows only some issues get; `sample` is the resting value the
// styleguide shows.

export type IssueMenuCondition =
  /** A phone: the long-press opened it, so "Select" starts a selection. */
  | `phone`
  /** An issue marked as a duplicate. */
  | `duplicate`
  /** The team estimates (`teams.estimation_type` other than `none`). */
  | `estimation`
  /** The team has more than one board to move to. */
  | `boards`

export type IssueMenuKey =
  | `open`
  | `toggle-done`
  | `copy-id`
  | `select`
  | `unmark-duplicate`
  | `status`
  | `assignee`
  | `priority`
  | `labels`
  | `estimate`
  | `due-date`
  | `move-board`
  | `add-relation`
  | `delete`

export interface IssueMenuLayoutRow {
  kind: `item` | `submenu` | `separator`
  key?: IssueMenuKey
  /** "Mark as done" reads "Move to backlog" on a completed issue. */
  label?: string
  icon?: MenuIcon
  destructive?: boolean
  sample?: string
  when?: IssueMenuCondition
}

/** @deprecated the old name of a layout row. */
export type IssueMenuEntry = IssueMenuLayoutRow

export const ISSUE_MENU_LAYOUT: readonly IssueMenuLayoutRow[] = [
  { kind: `item`, key: `open`, label: `Open issue`, icon: SquarePen },
  { kind: `item`, key: `toggle-done`, label: `Mark as done`, icon: CheckCheck },
  { kind: `item`, key: `copy-id`, label: `Copy issue ID`, icon: Copy },
  { kind: `item`, key: `select`, label: `Select`, icon: SquareCheckBig, when: `phone` },
  { kind: `item`, key: `unmark-duplicate`, label: `Unmark duplicate`, icon: Undo2, when: `duplicate` },
  { kind: `separator` },
  { kind: `submenu`, key: `status`, label: `Status`, icon: conceptIcon(`status-backlog`), sample: `Backlog` },
  { kind: `submenu`, key: `assignee`, label: `Assignee`, icon: conceptIcon(`ui-unassigned`), sample: `Unassigned` },
  { kind: `submenu`, key: `priority`, label: `Priority`, icon: conceptIcon(`priority-none`), sample: `No priority` },
  { kind: `submenu`, key: `labels`, label: `Labels`, icon: conceptIcon(`settings-labels`), sample: `None` },
  { kind: `submenu`, key: `estimate`, label: `Estimate`, icon: conceptIcon(`ui-estimate`), sample: `None`, when: `estimation` },
  { kind: `submenu`, key: `due-date`, label: `Set due date`, icon: conceptIcon(`ui-due-date`), sample: `None` },
  { kind: `submenu`, key: `move-board`, label: `Move to board`, icon: conceptIcon(`nav-boards`), sample: `App`, when: `boards` },
  { kind: `submenu`, key: `add-relation`, label: `Add relation`, icon: conceptIcon(`relation-section`) },
  // No separator above the destructive row (EXP-687): the red is the divider.
  { kind: `item`, key: `delete`, label: `Delete issue`, icon: conceptIcon(`ui-delete`), destructive: true },
]

function present(
  row: IssueMenuLayoutRow,
  conditions: ReadonlySet<IssueMenuCondition>
): boolean {
  return row.when === undefined || conditions.has(row.when)
}

/** The item labels, top to bottom, for an issue meeting `conditions`. */
export function issueMenuLabels(
  conditions: ReadonlySet<IssueMenuCondition>
): string[] {
  return ISSUE_MENU_LAYOUT.filter(
    (row) => row.kind !== `separator` && present(row, conditions)
  ).map((row) => row.label ?? ``)
}

/** What the live menu fills in per row: the verb, and anything that differs
 *  from the layout for THIS issue (a label, a glyph, the current value). */
export type IssueMenuSlot =
  | Partial<Omit<MenuItemEntry, `kind`>>
  | Partial<Omit<MenuSubmenuEntry, `kind`>>

/**
 * The layout as `MenuEntry[]` under the issue's header band. A row is drawn
 * when its `when` condition holds AND the caller filled its slot; separators
 * tidy themselves around the rows that drop out.
 */
export function issueMenuEntries({
  header,
  conditions,
  slots,
}: {
  header: { identifier: string; title: string }
  conditions: ReadonlySet<IssueMenuCondition>
  slots: Partial<Record<IssueMenuKey, IssueMenuSlot>>
}): MenuEntry[] {
  const entries: MenuEntry[] = [
    { kind: `header`, id: `header`, identifier: header.identifier, title: header.title },
  ]
  for (const row of ISSUE_MENU_LAYOUT) {
    if (row.kind === `separator`) {
      entries.push({ kind: `separator` })
      continue
    }
    const slot = row.key ? slots[row.key] : undefined
    if (!slot || !present(row, conditions)) continue
    const base = {
      id: row.key,
      label: row.label ?? ``,
      icon: row.icon,
      destructive: row.destructive,
    }
    if (row.kind === `item`) {
      entries.push({
        kind: `item`,
        onSelect: () => {},
        ...base,
        ...(slot as Partial<MenuItemEntry>),
      })
    } else {
      entries.push({ kind: `submenu`, ...base, ...(slot as Partial<MenuSubmenuEntry>) })
    }
  }
  return entries
}

/** The layout at rest, every row filled with its `sample` — the styleguide. */
export function issueMenuSampleEntries(
  conditions: ReadonlySet<IssueMenuCondition>,
  header: { identifier: string; title: string }
): MenuEntry[] {
  const slots: Partial<Record<IssueMenuKey, IssueMenuSlot>> = {}
  for (const row of ISSUE_MENU_LAYOUT) {
    if (!row.key) continue
    slots[row.key] =
      row.kind === `submenu` ? { value: row.sample, entries: [] } : { onSelect: () => {} }
  }
  return issueMenuEntries({ header, conditions, slots })
}
