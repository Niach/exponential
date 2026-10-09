import { useMemo } from "react"
import { contract } from "@exp/domain-contract"
import {
  Picker,
  FixConflictsCard as FixConflictsCardView,
  FixConflictsNote,
  FixConflictsPrRow,
  type PickerItem,
} from "@exp/ui"

import {
  useOpenPrOptions,
  usePrInputSeed,
} from "@/components/launch-dialog/action-input-fields"
import type { FixConflictsView } from "@/hooks/use-launch-composer"

// EXP-1233: the Fix merge conflicts builtin's CARD — what the composer's
// strip draws instead of the generic "Pull request" label + field, the
// DATA half. The pieces (the surface, the PR row, the refusal note) are
// `@exp/ui`'s `fix-conflicts-card.tsx`, the same ×4; this file binds the row
// to the team's open-PR picker (the same options the generic field lists,
// `useOpenPrOptions`) and hands it the contract's words.
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
// The note shows only when a refused merge brought us here (the seed's
// `conflict`); a pick made from the action picker has nothing to explain.

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
  const options = useMemo<PickerItem[]>(
    () => pulls.map((pull) => ({ value: pull.issueId, label: pull.label })),
    [pulls]
  )
  usePrInputSeed(pulls, seedIssueId, onChange)
  const { pr, refused } = view

  return (
    <FixConflictsCardView data-testid="agent-composer-fix-conflicts">
      <Picker
        mode="single"
        search
        items={options}
        value={value === `` ? null : value}
        onChange={onChange}
        width="lg"
        mobileTitle="Select a pull request"
        searchPlaceholder="Select a pull request..."
        emptyText="No open pull requests."
        disabled={disabled}
        trigger={({ open }) => (
          <FixConflictsPrRow
            pr={pr}
            placeholder={contract.composerUi.prPlaceholder}
            open={open}
          />
        )}
      />
      {refused && pr && (
        <FixConflictsNote data-testid="agent-composer-conflict-note">
          {contract.composerUi.conflictNote}
        </FixConflictsNote>
      )}
    </FixConflictsCardView>
  )
}
