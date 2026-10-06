import stackMergeChoice from "@exp/domain-contract/fixtures/stack-merge-choice.json"
import { conceptIcon } from "@exp/ui"

import { PromptSpecimen } from "./dialog-shared.tsx"
import type { StyleguideEntry } from "./types.ts"

// EXP-1145: the stack merge dialog, Cancel · Merge this pull request · Merge
// stack.
//
// The app's dialog (`apps/web/src/components/stack-merge-choice-dialog.tsx`)
// is the shared `Prompt` (EXP-1215), a Radix portal that renders nothing at
// rest, and no entry here imports an app composition. So the specimen draws
// the prompt's inside (`PromptSpecimen`) with the dialog's answers in order, and its words are the contract fixture's
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
  blurb: `What a plain Merge asks when its pull request is a member of an OPEN stack (EXP-1145), instead of the one-line confirm: the issue header, the Changes face, the review page, the run view and, since SLOP-3, every Reviews row. The title says so; the body lists the chain bottom to top joined by arrows, the pressed one marked "(this one)" and a batch pull request named with its count (\`EXP-874 +2\`), then one sentence per answer saying exactly what lands. Three answers, in this order: "${labels.cancel}", "${labels.mergeThis}" (a plain pill: the bottom member merges alone, any other one lands itself and everything below, and what sits above is retargeted onto the base branch and stays open), "${labels.mergeStack}" (the primary pill, the whole open chain, bottom-up). It is the shared prompt card (EXP-1215): one row of 32px pills, no ✕. The primary LEADS with the merge glyph on every client that draws button glyphs: web and the desktop alert (EXP-1167 settled it: the desktop alert was label-only). While a merge is in flight all three are disabled and the primary's glyph spins. A pull request with no other OPEN member in its chain never sees this dialog. The decision and every word are ONE pure function ×4 (\`stack-merge-choice.json\`).`,
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
      note: `a native alert whose primary wears the merge glyph (AlertSpec::ok_icon); the copy is domain pr_stack::stack_merge_choice`,
    },
    ios: {
      state: `ok`,
      symbol: `WorkMergePill`,
      file: `apps/ios/Exponential/UI/Work/WorkMergePill.swift`,
      note: `the shared GlassAlert over PrStack.stackMergeChoice; ReviewsView asks the same way`,
    },
    android: {
      state: `ok`,
      symbol: `StackMergeDialog`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/issue/ChangesScreen.kt`,
      note: `the shared GlassAlert over PrStack.stackMergeChoice`,
    },
  },
  island: () => (
    <PromptSpecimen
      className="max-w-lg"
      title={labels.title}
      body={<span className="whitespace-pre-line">{midStackBody()}</span>}
      actions={[
        { label: labels.cancel },
        { label: labels.mergeThis },
        { label: labels.mergeStack, role: `primary`, leading: <PrMergedIcon /> },
      ]}
    />
  ),
}
