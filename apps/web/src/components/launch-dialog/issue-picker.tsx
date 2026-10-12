import { useMemo, useState, type ReactNode } from "react"
import type { Issue } from "@/db/schema"
import { IssuePickerBody } from "@/components/issue-picker-host"
import { COMPOSER_MENU_TEST_IDS } from "@/components/launch-dialog/composer-menu"

// EXP-825: the composer's issue picker. Multi-select: a row toggles, checked
// rows pin to the top with their check, the rest is the codeable pool the
// hook derived (repo-backed boards, codeable anchor status, not running),
// ranked by the shared engine (EXP-892).
//
// EXP-1249: the composer's "+" menu hosts it as the "Implement issue"
// submenu body (a pushed page in the phone sheet). SLOP-28: the body is THE
// search list (`IssuePickerBody` → `@exp/ui` `SearchList`), so its rows are
// the `#` typeahead's and every issue picker's: status glyph, mono
// identifier, title, one height, eight visible, at the engine's default
// limit (the old 50-row cap is gone).

/** The searchable multi list; it stays put across toggles — a batch is
 *  several picks. `footer` = the Implement button once something is picked. */
export function ComposerIssuePicker({
  teamId,
  eligible,
  checked,
  onToggle,
  footer,
}: {
  /** The team `issues.search` runs against (EXP-892); undefined = local only. */
  teamId?: string
  /** The codeable pool, newest first (hook-derived). */
  eligible: Issue[]
  /** The checked issues' rows, pinned first. */
  checked: Issue[]
  onToggle: (issueId: string) => void
  footer?: ReactNode
}) {
  const [query, setQuery] = useState(``)
  const byId = useMemo(
    () => new Map(eligible.map((issue) => [issue.id, issue])),
    [eligible]
  )
  return (
    <div
      data-testid="agent-composer-issues-picker"
      className="flex max-h-[min(24rem,var(--radix-dropdown-menu-content-available-height,24rem))] min-h-0 flex-col"
    >
      <IssuePickerBody
        mode="multi"
        teamId={teamId}
        rows={eligible}
        checked={checked}
        onToggle={onToggle}
        query={query}
        onQueryChange={setQuery}
        // The pool is a subset of the team: a hit outside it is not codeable.
        resolveHit={(hit) => byId.get(hit.id) ?? null}
        searchPlaceholder="Search issues"
        inputTestId={COMPOSER_MENU_TEST_IDS.submenuSearch}
        footer={footer}
        emptyText={() =>
          eligible.length === 0
            ? `No codeable issues in repo-backed boards.`
            : `No matches. Only open issues are shown.`
        }
      />
    </div>
  )
}
