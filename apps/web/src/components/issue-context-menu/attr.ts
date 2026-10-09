import { menuProps, type MenuTargetProps } from "@exp/ui"

// EXP-1074 — how ANY element opts into THE issue context menu.
//
// Since the UI cleanup batch the gesture host is the generic `@exp/ui`
// `MenuGestureHost`, keyed by `data-menu` (the kind) + `data-menu-id`: an
// issue row, a sidebar row, a reviews row, an issue chip carry kind `issue`
// and the issue's id, and actions, runs and work tabs opt in the same way
// with their own kind. No wrapper, no handler props, nothing for a memoized
// row to compare: `grep issueMenuProps` is the whole surface.

export const ISSUE_MENU_KIND = `issue`

export type IssueMenuProps = MenuTargetProps

/** `from` is the `?from=` origin "Open issue" carries (EXP-851: the detail
 *  keeps the list it was opened from beside it). */
export function issueMenuProps(issueId: string, from?: string): IssueMenuProps {
  return menuProps(ISSUE_MENU_KIND, issueId, from)
}
