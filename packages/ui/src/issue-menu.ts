import {
  CheckCheck,
  Copy,
  SquareCheckBig,
  SquarePen,
  Undo2,
  type LucideIcon,
} from "lucide-react"

import { conceptIcon } from "./icons.generated"
import type { MenuSpecimenRow } from "./menu-specimen"

// EXP-1074 — THE issue context menu's layout, in one place.
//
// The web draws it live (apps/web/src/components/issue-context-menu, one host
// for every row and chip; its test locks the rendered item order to this
// list), the IDE mirrors it (crates/ui/src/issue_list.rs
// `build_row_context_menu`), and the styleguide draws it at rest
// (entries/issue-context-menu.tsx through `MenuSpecimen`). `when` names the
// rows only some issues get; `sample` is the resting value the specimen shows.

export type IssueMenuCondition =
  /** A phone: the long-press opened it, so "Select" starts a selection. */
  | `phone`
  /** An issue marked as a duplicate. */
  | `duplicate`
  /** The team estimates (`teams.estimation_type` other than `none`). */
  | `estimation`
  /** The team has more than one board to move to. */
  | `boards`

export interface IssueMenuEntry {
  kind: `item` | `submenu` | `separator`
  /** "Mark as done" reads "Move to backlog" on a completed issue. */
  label?: string
  icon?: LucideIcon
  destructive?: boolean
  sample?: string
  when?: IssueMenuCondition
}

export const ISSUE_MENU_LAYOUT: readonly IssueMenuEntry[] = [
  { kind: `item`, label: `Open issue`, icon: SquarePen },
  { kind: `item`, label: `Mark as done`, icon: CheckCheck },
  { kind: `item`, label: `Copy issue ID`, icon: Copy },
  { kind: `item`, label: `Select`, icon: SquareCheckBig, when: `phone` },
  { kind: `item`, label: `Unmark duplicate`, icon: Undo2, when: `duplicate` },
  { kind: `separator` },
  { kind: `submenu`, label: `Status`, icon: conceptIcon(`status-backlog`), sample: `Backlog` },
  { kind: `submenu`, label: `Assignee`, icon: conceptIcon(`ui-unassigned`), sample: `Unassigned` },
  { kind: `submenu`, label: `Priority`, icon: conceptIcon(`priority-none`), sample: `No priority` },
  { kind: `submenu`, label: `Labels`, icon: conceptIcon(`settings-labels`), sample: `None` },
  { kind: `submenu`, label: `Estimate`, icon: conceptIcon(`ui-estimate`), sample: `None`, when: `estimation` },
  { kind: `submenu`, label: `Set due date`, icon: conceptIcon(`ui-due-date`), sample: `None` },
  { kind: `submenu`, label: `Move to board`, icon: conceptIcon(`nav-boards`), sample: `App`, when: `boards` },
  { kind: `submenu`, label: `Add relation`, icon: conceptIcon(`relation-section`) },
  // No separator above the destructive row (EXP-687): the red is the divider.
  { kind: `submenu`, label: `Delete issue`, icon: conceptIcon(`ui-delete`), destructive: true },
]

function present(
  entry: IssueMenuEntry,
  conditions: ReadonlySet<IssueMenuCondition>
): boolean {
  return entry.when === undefined || conditions.has(entry.when)
}

/** The item labels, top to bottom, for an issue meeting `conditions`. */
export function issueMenuLabels(
  conditions: ReadonlySet<IssueMenuCondition>
): string[] {
  return ISSUE_MENU_LAYOUT.filter(
    (entry) => entry.kind !== `separator` && present(entry, conditions)
  ).map((entry) => entry.label ?? ``)
}

/** The layout as specimen rows, under the issue's header band. */
export function issueMenuSpecimenRows(
  conditions: ReadonlySet<IssueMenuCondition>,
  header: { identifier: string; title: string }
): MenuSpecimenRow[] {
  const rows: MenuSpecimenRow[] = [{ kind: `header`, ...header }, { kind: `separator` }]
  for (const entry of ISSUE_MENU_LAYOUT) {
    if (!present(entry, conditions)) continue
    if (entry.kind === `separator`) {
      rows.push({ kind: `separator` })
    } else if (entry.kind === `item`) {
      rows.push({ kind: `item`, label: entry.label ?? ``, icon: entry.icon, destructive: entry.destructive })
    } else {
      rows.push({
        kind: `submenu`,
        label: entry.label ?? ``,
        icon: entry.icon,
        destructive: entry.destructive,
        value: entry.sample,
      })
    }
  }
  return rows
}
