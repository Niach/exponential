import stackMergeChoice from "@exp/domain-contract/fixtures/stack-merge-choice.json"
import { conceptIcon } from "@exp/ui"

import { PromptSpecimen } from "./dialog-shared.tsx"
import type { StyleguideEntry } from "./types.ts"

// EXP-1248: the ONE stack merge confirm (it replaced EXP-1145's 3-way
// dialog). The app's dialog (`apps/web/src/components/stack-merge-choice-dialog.tsx`
// `StackMergeConfirmDialog`) is the shared `Prompt`, a Radix portal that
// renders nothing at rest, and no entry imports an app composition. So the
// specimens draw the prompt's inside (`PromptSpecimen`) with the words of the
// contract fixture's `confirm` cases (`stack-merge-choice.json`, ×4): Merge
// stack from a member, and Merge through here on the middle of three.

const { confirm } = stackMergeChoice

const PrMergedIcon = conceptIcon(`pr-merged`)

function confirmCase(mode: `stack` | `through`, staysOpen: number) {
  const found = confirm.cases.find(
    (row) =>
      row.mode === mode &&
      row.confirm !== null &&
      row.confirm.landing.length > 1 &&
      row.confirm.staysOpen.length === staysOpen
  )
  if (!found?.confirm) {
    throw new Error(`stack-merge-choice.json has no ${mode} confirm case`)
  }
  return found.confirm
}

function Specimen({ mode, staysOpen }: { mode: `stack` | `through`; staysOpen: number }) {
  const words = confirmCase(mode, staysOpen)
  return (
    <PromptSpecimen
      className="max-w-lg"
      title={words.title}
      body={words.body}
      actions={[
        { label: confirm.labels.cancel },
        { label: words.title, role: `primary`, leading: <PrMergedIcon /> },
      ]}
    />
  )
}

export const entry: StyleguideEntry = {
  id: `stack-merge-choice-dialog`,
  section: `special`,
  owner: `EXP-1145`,
  title: `Stack merge confirm`,
  blurb: `What a merge on a member of an OPEN linear stack asks (EXP-1248, replacing EXP-1145's three answers). ONE merge control: on any stack member it reads "${confirm.labels.mergeStack}" and lands the whole open chain (through its top); hovering a member row of the stack rail shows a ghost "${confirm.labels.mergeThrough}" (a phone row's long-press menu) that lands that member and every open member beneath it. Either opens this confirm: the title is the action, the body says what lands bottom-up ("Lands 2 pull requests, bottom-up: …") and what stays open above it (GitHub retargets it), a batch pull request named with its count (\`EXP-874 +2\`). Two answers: "${confirm.labels.cancel}" and the primary pill repeating the title, leading with the merge glyph. The server lands it with ONE GitHub merge-async on that member, after making the line a GitHub stack; a PR TREE (any fork) never gets this confirm (its root merges plainly first). The decision and every word are ONE pure function ×4 (\`stack-merge-choice.json\` \`confirm\`).`,
  status: {
    web: {
      state: `ok`,
      symbol: `StackMergeConfirmDialog`,
      file: `apps/web/src/components/stack-merge-choice-dialog.tsx`,
      note: `copy = lib/pr-stack.ts stackMergeConfirm; the Merge capsule and the Guide stack rail open it`,
    },
    desktop: {
      state: `ok`,
      symbol: `pr_merge::ask_stack_merge_mode`,
      file: `apps/desktop/crates/ui/src/pr_merge.rs`,
      note: `stack_confirm_alert over domain pr_stack::stack_merge_confirm`,
    },
    ios: {
      state: `ok`,
      symbol: `WorkMergePill.stackMergeConfirmAlert`,
      file: `apps/ios/Exponential/UI/Work/WorkMergePill.swift`,
      note: `copy = ExpCore PrStack.stackMergeConfirm`,
    },
    android: {
      state: `ok`,
      symbol: `StackMergeDialog`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/work/StackMergeDialog.kt`,
      note: `copy = domain PrStack.stackMergeConfirm`,
    },
  },
  island: () => (
    <div className="flex flex-col gap-4">
      <Specimen mode="stack" staysOpen={0} />
      <Specimen mode="through" staysOpen={1} />
    </div>
  ),
}
