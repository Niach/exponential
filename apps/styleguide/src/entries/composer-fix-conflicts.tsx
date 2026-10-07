import { contract } from "@exp/domain-contract"
import {
  BUILTIN_STATUS_COLOR_CLASS,
  categoryStatusIcon,
  FixConflictsCard,
  FixConflictsNote,
  FixConflictsPrRow,
  IssueChip,
} from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1233: the composer's Fix merge conflicts look — what a merge refused
// by a REAL conflict opens, on every client, in place of the old "Fix
// conflicts" button swap.
//
// An island of the REAL pieces: the headline is the contract's verb beside
// the pull request's `IssueChip` (the one the issues subject draws, ✕ and
// all), the card is `@exp/ui`'s `FixConflictsCard` — the PR row (the
// picker's trigger) and the refusal note. The composer card under it and the
// options line are the `composer-dialog` entry's and are not repeated here.

const { composerUi } = contract

const PR = { prNumber: 2117, branch: `exp/APP-14`, baseBranch: `master` }

export const entry: StyleguideEntry = {
  id: `composer-fix-conflicts`,
  section: `special`,
  owner: `EXP-1233`,
  title: `Composer · Fix merge conflicts`,
  blurb: `What a Merge refused by a REAL conflict opens: the launcher on the Fix merge conflicts builtin with THAT pull request picked, at once — no "Fix conflicts" button parked in the Merge slot, no "Retry merge" secondary, no caption (Merge stays plain Merge, so a conflict fixed elsewhere is one click away). Every other refusal stays a toast or row caption. Once a pull request is picked the builtin wears its own look (×4): the headline reads "${composerUi.fixConflictsHeadline}" followed by the PR's issue chips (a batch PR shows each of its issues; the ✕ clears the PICK, not the action), and the strip is this card in place of the generic "Pull request" field — one row with the open-PR glyph, the number in mono, the branch glyph and \`branch → base\` at 70% (the base omitted while unknown), the chevron saying the row IS the picker; under a hairline, only when a refused merge brought you here, the warning glyph and "${composerUi.conflictNote}" in the destructive tone. The send is named "${composerUi.fixConflictsSubmit}". Picked from the action picker instead, the composer starts as the plain "Run" + action chip with the row reading "${composerUi.prPlaceholder}" and completes into this look on the pick. Words = contract composerUi; the seed carries \`conflict\` (web \`?conflict=1\`).`,
  status: {
    web: {
      state: `ok`,
      symbol: `FixConflictsCard / FixConflictsPrRow / FixConflictsNote`,
      file: `packages/ui/src/fix-conflicts-card.tsx`,
      note: `bound in apps/web/src/components/launch-dialog/fix-conflicts-card.tsx; the headline in launch-headline.tsx; SessionMergeButton and the Reviews rows open it`,
    },
    desktop: {
      state: `ok`,
      symbol: `chat_screen::render_fix_conflicts_card`,
      file: `apps/desktop/crates/ui/src/chat_screen.rs`,
      note: `work_header::conflict_opens_composer + reviews_view open it; styleguide/entries/composer_fix_conflicts.rs`,
    },
    ios: {
      state: `ok`,
      symbol: `FixConflictsCard`,
      file: `apps/ios/Exponential/UI/Agent/FixConflictsCard.swift`,
      note: `WorkMergePill and ReviewsView open it with AgentComposerSeed(conflict: true)`,
    },
    android: {
      state: `ok`,
      symbol: `FixConflictsCard`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/agent/FixConflictsCard.kt`,
      note: `WorkScreen and ReviewsScreen open it with AgentComposerSeed(conflict = true)`,
    },
  },
  island: () => (
    <div className="flex max-w-xl flex-col gap-2">
      <div className="flex flex-wrap items-center gap-x-2 gap-y-1.5 px-1 text-lg leading-tight font-semibold">
        <span className="shrink-0">{composerUi.fixConflictsHeadline}</span>
        <IssueChip
          identifier="APP-14"
          title="Group board issues by assignee"
          // In review = the second of two started statuses (the clock at 2/4).
          status={{
            icon: categoryStatusIcon(`started`, 1, 2),
            colorClass: BUILTIN_STATUS_COLOR_CLASS.in_review,
          }}
          onRemove={() => {}}
          removeLabel="Clear the pull request"
        />
      </div>
      <FixConflictsCard>
        <FixConflictsPrRow pr={PR} placeholder={composerUi.prPlaceholder} />
        <FixConflictsNote>{composerUi.conflictNote}</FixConflictsNote>
      </FixConflictsCard>
      <FixConflictsCard>
        <FixConflictsPrRow pr={null} placeholder={composerUi.prPlaceholder} />
      </FixConflictsCard>
    </div>
  ),
}
