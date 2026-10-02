import fixture from "@exp/domain-contract/fixtures/detail-chrome.json"

// EXP-1162: the detail chrome, locked ×4 against the ONE contract fixture
// (desktop domain::detail_chrome, iOS ExpCore DetailChrome, Android
// domain/DetailChrome): the issue title is a ROW of the scrolling content and
// BREAKS into the header once it has scrolled away; content runs under the
// header band and the bottom bar behind a thin blurred fade.

/** Points (web px): the collapse timing and the two edge strips. */
export const DETAIL_CHROME = fixture.constants

/**
 * Whether the header shows the collapsed title (identifier over the title on
 * one line). A threshold, never a progressive morph: the title row's bottom
 * edge reaching the header band's. A face with no title row of its own (Run,
 * Changes, Results) is always collapsed; a row not measured yet never is.
 */
export function isTitleCollapsed(
  hasTitleRow: boolean,
  titleBottom: number | null,
  headerBottom: number
): boolean {
  if (!hasTitleRow) return true
  if (titleBottom === null) return false
  return titleBottom <= headerBottom
}
