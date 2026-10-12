import { useRef, useState } from "react"
import { SearchListPopover, type TypeaheadAnchor } from "@exp/ui"
import {
  useIssueRefs,
  type ResolvedIssueRef,
} from "@/components/issue-ref-provider"
import { IssuePickerBody } from "@/components/issue-picker-host"
import { ISSUE_SEARCH_NO_RESULTS, ISSUE_SEARCH_PLACEHOLDER } from "@/lib/issue-search"

// SLOP-28 (EXP-1259 D4, search-list.json): THE `#` typeahead, one component
// for every web editor that has one (the TipTap markdown editor and the
// MentionTextarea every composer rides: comments, steering, the launch
// composer). Typing `#` opens THE search list at the caret (a bottom sheet on
// a phone) with ITS input focused and prefilled with the token typed after
// the `#` (`typeaheadPrefill`), spaces allowed; the rows are the issue
// picker's, ranked by the shared engine at its DEFAULT limit (the editors'
// old 6-row cap is gone). The host owns the text: a pick replaces `#token`
// with `#IDENT `, Escape leaves it as typed, Backspace in the empty input
// deletes the `#`; focus goes back to the host editor after each.

export interface IssueRefSearchRequest {
  /** Offset (or doc position) of the `#`: one open per trigger. */
  start: number
  /** The token after the `#`: the input's prefill. */
  query: string
  /** The caret rect (pointer popover); null = not measurable yet. */
  anchor: TypeaheadAnchor | null
}

export function IssueRefSearch({
  request,
  onPick,
  onDismiss,
  onBackspaceEmpty,
  onReturnFocus,
}: {
  /** Null = closed. */
  request: IssueRefSearchRequest | null
  onPick: (issue: ResolvedIssueRef) => void
  /** Escape or a click elsewhere: the text stays as typed. */
  onDismiss: () => void
  /** Backspace in the empty input: the host deletes the `#` token. */
  onBackspaceEmpty: () => void
  /** Focus back into the host editor once the surface closed. */
  onReturnFocus: () => void
}) {
  const issueRefs = useIssueRefs()
  const [query, setQuery] = useState(request?.query ?? ``)
  // A new `#` re-seeds the input with its own token (derived state: reset
  // during render so the first frame already shows the prefill).
  const seeded = useRef<number | null>(request?.start ?? null)
  const seedKey = request?.start ?? null
  if (seeded.current !== seedKey) {
    seeded.current = seedKey
    setQuery(request?.query ?? ``)
  }

  if (issueRefs === null) return null
  const open = request !== null
  return (
    <SearchListPopover
      open={open}
      onOpenChange={(next) => {
        if (!next) onDismiss()
      }}
      anchor={request?.anchor ?? null}
      title="Issues"
      onReturnFocus={onReturnFocus}
      data-testid="issue-ref-search"
    >
      <IssuePickerBody<ResolvedIssueRef>
        mode="single"
        teamId={issueRefs.teamId}
        rows={issueRefs.rows}
        resolveHit={(hit) => issueRefs.fromHit(hit)}
        server={open}
        query={query}
        onQueryChange={setQuery}
        searchPlaceholder={ISSUE_SEARCH_PLACEHOLDER}
        emptyText={() => ISSUE_SEARCH_NO_RESULTS}
        onBackspaceEmpty={onBackspaceEmpty}
        onPick={onPick}
      />
    </SearchListPopover>
  )
}
