import stackMergeChoice from "@exp/domain-contract/fixtures/stack-merge-choice.json"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  Button,
  conceptIcon,
} from "@exp/ui"

import { DialogSpecimen } from "./dialog-shared.tsx"
import type { StyleguideEntry } from "./types.ts"

// EXP-1145: the stack merge dialog, Cancel · Merge this pull request · Merge
// stack.
//
// The app's dialog (`apps/web/src/components/stack-merge-choice-dialog.tsx`)
// is an alert dialog, a Radix portal that renders nothing at rest, and no
// entry here imports an app composition. So the specimen draws the dialog's
// REAL `@exp/ui` parts in its order, and its words are the contract fixture's
// (`stack-merge-choice.json`, byte-locked ×4): the labels and the body of the
// "middle of a three-stack" case, the one that shows both sentences at work.

const { labels, cases } = stackMergeChoice

const PrMergedIcon = conceptIcon(`pr-merged`)

/** The fixture's body for a merge pressed mid-stack. */
function midStackBody(): string {
  const found = cases.find(
    (row) => row.choice !== null && row.choice.position === 2 && row.choice.members.length === 3
  )
  if (!found?.choice) {
    throw new Error(`stack-merge-choice.json has no middle-of-three case`)
  }
  return found.choice.body
}

export const entry: StyleguideEntry = {
  id: `stack-merge-choice-dialog`,
  section: `special`,
  owner: `EXP-1145`,
  title: `Stack merge dialog`,
  blurb: `What a plain Merge asks when its pull request is a member of an OPEN stack (EXP-1145), instead of the one-line confirm: the issue header, the Changes face, the review page, the run view and, since SLOP-3, every Reviews row. The title says so; the body lists the chain bottom to top joined by arrows, the pressed one marked "(this one)" and a batch pull request named with its count (\`EXP-874 +2\`), then one sentence per answer saying exactly what lands. Three answers, in this order: "${labels.cancel}", "${labels.mergeThis}" (outline: the bottom member merges alone, any other one lands itself and everything below, and what sits above is retargeted onto the base branch and stays open), "${labels.mergeStack}" (primary, with the merge glyph: the whole open chain, bottom-up). While a merge is in flight all three are disabled and the primary's glyph spins. A pull request with no other OPEN member in its chain never sees this dialog. The decision and every word are ONE pure function ×4 (\`stack-merge-choice.json\`).`,
  status: {
    web: {
      state: `ok`,
      symbol: `StackMergeChoiceDialog`,
      file: `apps/web/src/components/stack-merge-choice-dialog.tsx`,
      note: `the copy is lib/pr-stack.ts stackMergeChoice; SessionMergeButton and the Reviews rows open it`,
    },
    desktop: {
      state: `ok`,
      symbol: `pr_merge::ask_stack_merge`,
      file: `apps/desktop/crates/ui/src/pr_merge.rs`,
      note: `a native alert; the copy is domain pr_stack::stack_merge_choice`,
    },
    ios: {
      state: `ok`,
      symbol: `WorkMergePill`,
      file: `apps/ios/Exponential/UI/Work/WorkMergePill.swift`,
      note: `a stock confirmationDialog over PrStack.stackMergeChoice; ReviewsView asks the same way`,
    },
    android: {
      state: `ok`,
      symbol: `StackMergeDialog`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/issue/ChangesScreen.kt`,
      note: `an M3 AlertDialog over PrStack.stackMergeChoice`,
    },
  },
  island: () => (
    // The Radix root gives the title, the description and the action their
    // context. Cancel is the outline button `AlertDialogCancel` paints: the
    // real one only mounts inside the portalled content.
    <AlertDialog open>
      <DialogSpecimen className="max-w-md">
        <AlertDialogHeader>
          <AlertDialogTitle>{labels.title}</AlertDialogTitle>
          <AlertDialogDescription className="whitespace-pre-line">
            {midStackBody()}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <Button variant="outline">{labels.cancel}</Button>
          <Button variant="outline">{labels.mergeThis}</Button>
          <AlertDialogAction>
            <PrMergedIcon />
            {labels.mergeStack}
          </AlertDialogAction>
        </AlertDialogFooter>
      </DialogSpecimen>
    </AlertDialog>
  ),
}
