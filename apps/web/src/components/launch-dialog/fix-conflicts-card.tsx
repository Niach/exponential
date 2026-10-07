import { useMemo } from "react"
import { contract } from "@exp/domain-contract"
import { Combobox, conceptIcon, GlassGroup, type PickerOption } from "@exp/ui"

import {
  useOpenPrOptions,
  usePrInputSeed,
} from "@/components/launch-dialog/action-input-fields"
import type { FixConflictsView } from "@/hooks/use-launch-composer"
import { cn } from "@/lib/utils"

// EXP-1233: the Fix merge conflicts builtin's CARD — what the composer's
// strip draws instead of the generic "Pull request" label + field.
//
// A refused merge used to swap the Merge button for a "Fix conflicts" one
// and leave the person to press it; now the refusal opens the composer
// straight away (`session-merge-button.tsx`, the Reviews rows, ×4), so the
// composer has to say, at a glance, what it is about to do and to what:
//
//   Fix merge conflicts  [APP-14 Group board issues by assignee ×]   ← headline
//   ┌ ⑂ #2117  exp/APP-14 → master                              ⌄ ┐  ← this card
//   │ ⚠ Merge refused: the branch has conflicts.                   │
//   └──────────────────────────────────────────────────────────────┘
//
// The branch row IS the PR picker (the same open-PR options the generic
// field lists; the chevron says so), so re-picking is one click and the
// unpicked state is the picker alone ("Select a pull request…"). The
// conflict line shows only when a refused merge brought us here (the seed's
// `conflict`); a pick made from the action picker has nothing to explain.
// Copy = `contract.composerUi` (×4).

const PrOpenIcon = conceptIcon(`pr-open`)
const UiBranchIcon = conceptIcon(`ui-branch`)
const UiWarningIcon = conceptIcon(`ui-warning`)
const ChevronIcon = conceptIcon(`ui-chevron-down`)

/** `exp/APP-14 → master`, or the branch alone while the base is unknown.
 *  Pure, so the arrow rule is a test (the natives print the same line). */
export function branchLine(
  branch: string | null,
  baseBranch: string | null
): string {
  if (!branch) return ``
  return baseBranch ? `${branch} → ${baseBranch}` : branch
}

export function FixConflictsCard({
  view,
  teamId,
  value,
  seedIssueId,
  onChange,
  disabled,
}: {
  view: FixConflictsView
  teamId: string
  /** The `pr` input's value — the representative issue id, or ``. */
  value: string
  /** Any issue linked to the PR to open pre-picked (a refused merge's). */
  seedIssueId?: string
  onChange: (issueId: string) => void
  disabled?: boolean
}) {
  const pulls = useOpenPrOptions(teamId)
  const options = useMemo<PickerOption[]>(
    () => pulls.map((pull) => ({ value: pull.issueId, label: pull.label })),
    [pulls]
  )
  usePrInputSeed(pulls, seedIssueId, onChange)
  const { pr, refused } = view
  const line = pr ? branchLine(pr.branch, pr.baseBranch) : ``

  return (
    <GlassGroup data-testid="agent-composer-fix-conflicts">
      <Combobox
        options={options}
        value={value === `` ? null : value}
        onChange={(issueId) => onChange(issueId ?? ``)}
        width="lg"
        mobileTitle="Select a pull request"
        placeholder="Select a pull request..."
        emptyText="No open pull requests."
        disabled={disabled}
        renderTrigger={({ open }) => (
          <button
            type="button"
            data-slot="fix-conflicts-pr-row"
            data-state={open ? `open` : `closed`}
            aria-label={
              pr
                ? `Pull request ${pr.prNumber ? `#${pr.prNumber}` : ``}`.trim()
                : contract.composerUi.prPlaceholder
            }
            className={cn(
              `flex w-full items-center gap-3 px-4 py-3 text-left text-sm transition-colors duration-fast outline-none`,
              `hover:bg-glass-active/50 focus-visible:ring-[3px] focus-visible:ring-inset focus-visible:ring-ring/50`,
              `disabled:pointer-events-none disabled:opacity-50`
            )}
          >
            {pr ? (
              <>
                <PrOpenIcon
                  aria-hidden
                  className="size-4 shrink-0 text-emerald-500"
                />
                {pr.prNumber !== null && (
                  <span className="shrink-0 font-mono text-foreground">
                    #{pr.prNumber}
                  </span>
                )}
                <span className="flex min-w-0 items-center gap-1.5 font-mono text-foreground/70">
                  <UiBranchIcon aria-hidden className="size-3.5 shrink-0" />
                  <span className="min-w-0 truncate">{line}</span>
                </span>
              </>
            ) : (
              <span className="min-w-0 flex-1 truncate text-muted-foreground">
                {contract.composerUi.prPlaceholder}
              </span>
            )}
            <ChevronIcon
              aria-hidden
              className="ml-auto size-3.5 shrink-0 text-foreground/50"
            />
          </button>
        )}
      />
      {refused && pr && (
        <div
          className="flex items-center gap-2 px-4 py-2.5 text-sm text-destructive"
          data-testid="agent-composer-conflict-note"
        >
          <UiWarningIcon aria-hidden className="size-4 shrink-0" />
          <span className="min-w-0 truncate">
            {contract.composerUi.conflictNote}
          </span>
        </div>
      )}
    </GlassGroup>
  )
}
