import issueDraft from "@exp/domain-contract/fixtures/issue-draft.json"
import { Dialog, DialogFooter, DialogTitle, Pill } from "@exp/ui"

import { DialogSpecimen } from "./dialog-shared.tsx"
import type { StyleguideEntry } from "./types.ts"

// EXP-1212: the New issue page's two prompts — the `×`'s destructive discard
// confirm, and the three-choice dialog a HELD navigation asks.
//
// The app's dialogs (`apps/web/src/components/issue-draft-page.tsx`) are
// Radix portals that render nothing at rest, and no entry here imports an
// app composition. So the specimen draws their REAL `@exp/ui` parts in the
// page's order, and every word is the contract fixture's `copy` (the page's
// `ISSUE_DRAFT_COPY` IS `issue-draft.json`'s `copy`, byte-locked ×4).

const { copy } = issueDraft

// The page's DRAFT_PROMPT_* classes (issue-draft-page.tsx), byte for byte.
const CARD = `max-w-md p-5`
const ROW = `flex-row flex-wrap items-center justify-end`
const DESTRUCTIVE_PILL = `border-destructive/40 text-destructive hover:text-destructive`
const QUIET_DISCARD = `-ml-3 mr-auto border-transparent bg-transparent text-destructive hover:bg-transparent hover:text-destructive`

export const entry: StyleguideEntry = {
  id: `draft-leave-dialog`,
  section: `special`,
  owner: `EXP-1212`,
  title: `Draft leave dialog`,
  blurb: `What leaving a New issue draft WITH content asks (EXP-1212; a title, a description or an attachment is content, and a reopened draft whose files are not known yet counts as content). Each prompt is ONE question (no body line, no ✕; Esc and the scrim dismiss) over ONE row of the app's 32px \`Pill\` capsules, the same capsules the natives' alert card draws. Its close button first asks "${copy.discardConfirm.title}": Cancel and "${copy.discardConfirm.confirm}" (the plain pill with a destructive label and tinted border, no fill). Every other way off the page (back, a nav entry, another screen, closing the IDE tab) is HELD and asks "${copy.leave.title}", Thunderbird's save prompt: "${copy.leave.discard}" set apart on the leading edge (quiet destructive text, no second confirm), then "${copy.leave.keep}" (the plain pill: save, then continue; a failed save stays on the page with the save error) and the default "${copy.leave.create}" (the primary pill, trailing, initial focus and Enter; the held navigation continues instead of opening the new issue; disabled without a title, then "${copy.leave.keep}" takes the focus). Dismissing stays. Nothing is asked while a Create is in flight, and a draft with no content leaves silently. A mode that never writes a draft row (a sub-issue or share compose on phones) offers Discard and Create issue only. The copy is the contract's (\`issue-draft.json\`, byte-locked ×4).`,
  status: {
    web: {
      state: `ok`,
      symbol: `IssueDraftPage`,
      file: `apps/web/src/components/issue-draft-page.tsx`,
      note: `a router useBlocker holds the navigation; the copy is lib/issue-draft-page.ts ISSUE_DRAFT_COPY`,
    },
    desktop: {
      state: `ok`,
      symbol: `issue_draft_screen::prompt_leave`,
      file: `apps/desktop/crates/ui/src/issue_draft_screen.rs`,
      note: `native alerts (AlertSpec); the copy is domain issue_draft`,
    },
    ios: {
      state: `ok`,
      symbol: `IssueDraftPageView`,
      file: `apps/ios/Exponential/UI/Issue/IssueDraftPageView.swift`,
      note: `ExpUI GlassAlert (Pill row); IssueDraftLeaveGuard holds the navigation`,
    },
    android: {
      state: `ok`,
      symbol: `IssueDraftScreen`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/issue/IssueDraftScreen.kt`,
      note: `GlassAlert (Pill row); navigation/LeaveGuard holds the navigation`,
    },
  },
  island: () => (
    <div className="grid gap-6">
      {/* Both prompts are the page's ONE structure: a \`Dialog\` card with the
          DRAFT_PROMPT_* classes (one question, no body, no ✕, 20px padding)
          over ONE row of \`size="md"\` action pills. */}
      <Dialog open>
        <DialogSpecimen caption="The close button, a draft with content" className={CARD}>
          <DialogTitle>{copy.discardConfirm.title}</DialogTitle>
          <DialogFooter className={ROW}>
            <Pill size="md" mode="action">
              Cancel
            </Pill>
            <Pill size="md" mode="action" className={DESTRUCTIVE_PILL}>
              {copy.discardConfirm.confirm}
            </Pill>
          </DialogFooter>
        </DialogSpecimen>
      </Dialog>
      <Dialog open>
        <DialogSpecimen caption="Any other way off the page, held" className={CARD}>
          <DialogTitle>{copy.leave.title}</DialogTitle>
          <DialogFooter className={ROW}>
            <Pill size="md" mode="action" className={QUIET_DISCARD}>
              {copy.leave.discard}
            </Pill>
            <Pill size="md" mode="action">
              {copy.leave.keep}
            </Pill>
            <Pill size="md" mode="action" primary>
              {copy.leave.create}
            </Pill>
          </DialogFooter>
        </DialogSpecimen>
      </Dialog>
    </div>
  ),
}
