import type { LucideIcon } from "lucide-react"
import type { ReactNode } from "react"

import {
  Picker,
  type PickerItem,
  type PickerSurfaceProps,
} from "./picker"

// EXP-1029 contract — the issue picker (single or multi): rows read
// `IDENT Title` behind the issue's STATUS glyph, search matches the
// identifier and the title. The composer
// picks several (a batch); relations, duplicates and the stack dialog pick
// one. Search over a big board goes through `useIssueSearchResults`
// (EXP-892) at the call site; the picker only renders what it is handed.

export interface IssuePickerIssue {
  id: string
  identifier: string
  title: string
  /** The issue's resolved STATUS glyph (`lib/status-icons.ts` on the web, its
   *  twin on each native). Optional only so a list with no team statuses to
   *  hand can still render; every real call site resolves it, because the
   *  relations linker — the look EXP-1021 is measured against — leads each
   *  row with it. The one-line `IDENT Title` label is unchanged. */
  icon?: LucideIcon
  /** The glyph's colour: a builtin status' Tailwind token class, or a custom
   *  row's hex. */
  color?: string
  disabled?: boolean
}

interface IssuePickerBase extends PickerSurfaceProps {
  issues: readonly IssuePickerIssue[]
  trigger: ReactNode
  /** The sheet's title on a phone. */
  mobileTitle?: string
  search?: boolean
  disabled?: boolean
  emptyText?: ReactNode
  className?: string
  /** Controlled search text for the shared ranking engine
   *  (`useIssueSearchResults`, EXP-892): pass `shouldFilter={false}` with it
   *  so the caller's order is rendered verbatim. */
  query?: string
  onQueryChange?: (query: string) => void
  shouldFilter?: boolean
  loading?: boolean
  /** Replaces the row BODY — the surfaces that keep the identifier in its own
   *  mono column (the relations linker). The selection language stays the
   *  primitive's. */
  renderItem?: (
    item: PickerItem,
    state: { selected: boolean }
  ) => ReactNode
}

export type IssuePickerProps = IssuePickerBase &
  (
    | { mode?: `single`; value: string | null; onChange: (issueId: string) => void }
    | {
        mode: `multi`
        value: readonly string[]
        onChange: (issueIds: string[]) => void
        /** At the cap the unpicked rows go disabled; picked ones still
         *  toggle off (the automation trigger's ten-id filters). */
        max?: number
      }
  )

export function issuePickerItems(issues: readonly IssuePickerIssue[]): PickerItem[] {
  return issues.map((issue) => ({
    value: issue.id,
    label: `${issue.identifier} ${issue.title}`,
    keywords: [issue.identifier, issue.title],
    icon: issue.icon,
    color: issue.color,
    disabled: issue.disabled,
  }))
}

export function IssuePicker({
  issues,
  emptyText = `No issues`,
  mobileTitle = `Issues`,
  search = true,
  ...props
}: IssuePickerProps) {
  const items = issuePickerItems(issues)
  const shared = { items, emptyText, mobileTitle, search }
  if (props.mode === `multi`) {
    const { mode: _mode, ...rest } = props
    return <Picker mode="multi" {...shared} {...rest} />
  }
  const { mode: _mode, ...rest } = props
  return <Picker mode="single" {...shared} {...rest} />
}
