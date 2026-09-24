import type { ReactNode } from "react"

import { Picker, type PickerItem } from "./picker"

// EXP-1029 contract — the issue picker (single or multi): rows read
// `IDENT Title`, search matches the identifier and the title. The composer
// picks several (a batch); relations, duplicates and the stack dialog pick
// one. Search over a big board goes through `useIssueSearchResults`
// (EXP-892) at the call site; the picker only renders what it is handed.

export interface IssuePickerIssue {
  id: string
  identifier: string
  title: string
  disabled?: boolean
}

interface IssuePickerBase {
  issues: readonly IssuePickerIssue[]
  trigger: ReactNode
  /** The sheet's title on a phone. */
  mobileTitle?: string
  search?: boolean
  disabled?: boolean
  emptyText?: string
  className?: string
}

export type IssuePickerProps = IssuePickerBase &
  (
    | { mode?: `single`; value: string | null; onChange: (issueId: string) => void }
    | { mode: `multi`; value: readonly string[]; onChange: (issueIds: string[]) => void }
  )

export function issuePickerItems(issues: readonly IssuePickerIssue[]): PickerItem[] {
  return issues.map((issue) => ({
    value: issue.id,
    label: `${issue.identifier} ${issue.title}`,
    keywords: [issue.identifier, issue.title],
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
