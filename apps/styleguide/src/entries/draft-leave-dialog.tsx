import issueDraft from "@exp/domain-contract/fixtures/issue-draft.json"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  Button,
  Dialog,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@exp/ui"

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

export const entry: StyleguideEntry = {
  id: `draft-leave-dialog`,
  section: `special`,
  owner: `EXP-1212`,
  title: `Draft leave dialog`,
  blurb: `What leaving a New issue draft WITH content asks (EXP-1212; a title, a description or an attachment is content, and a reopened draft whose files are not known yet counts as content). Its close button first asks the destructive discard confirm: "${copy.discardConfirm.title}", the platform's Cancel and a destructive "${copy.discardConfirm.confirm}". Every other way off the page (back, a nav entry, another screen, closing the IDE tab) is HELD and asks "${copy.leave.title}" with three answers, in this order: "${copy.leave.discard}" (destructive, no second confirm), "${copy.leave.keep}" (outline: save, then continue; a failed save stays on the page with the save error) and "${copy.leave.create}" (primary, disabled without a title; the held navigation continues instead of opening the new issue). Initial keyboard focus sits on "${copy.leave.keep}", never on "${copy.leave.discard}"; dismissing stays. Nothing is asked while a Create is in flight, and a draft with no content leaves silently. A mode that never writes a draft row (a sub-issue or share compose on phones) offers Create and Discard only. The copy is the contract's (\`issue-draft.json\`, byte-locked ×4).`,
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
      note: `an alert and a confirmationDialog; IssueDraftLeaveGuard holds the navigation`,
    },
    android: {
      state: `ok`,
      symbol: `IssueDraftScreen`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/issue/IssueDraftScreen.kt`,
      note: `two M3 AlertDialogs; navigation/LeaveGuard holds the navigation`,
    },
  },
  island: () => (
    <div className="grid gap-6">
      {/* The Radix roots give the titles, descriptions and the action their
          context. Cancel is the outline button `AlertDialogCancel` paints:
          the real one only mounts inside the portalled content. */}
      <AlertDialog open>
        <DialogSpecimen caption="The close button, a draft with content" className="max-w-md">
          <AlertDialogHeader>
            <AlertDialogTitle>{copy.discardConfirm.title}</AlertDialogTitle>
            <AlertDialogDescription>{copy.discardConfirm.body}</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <Button variant="outline">Cancel</Button>
            <AlertDialogAction className="bg-destructive text-white hover:bg-destructive/90">
              {copy.discardConfirm.confirm}
            </AlertDialogAction>
          </AlertDialogFooter>
        </DialogSpecimen>
      </AlertDialog>
      <Dialog open>
        <DialogSpecimen caption="Any other way off the page, held" showClose>
          <DialogHeader>
            <DialogTitle>{copy.leave.title}</DialogTitle>
            <DialogDescription>{copy.leave.body}</DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="destructive"
              className="bg-destructive text-white hover:bg-destructive/90 dark:bg-destructive"
            >
              {copy.leave.discard}
            </Button>
            <Button variant="outline">{copy.leave.keep}</Button>
            <Button>{copy.leave.create}</Button>
          </DialogFooter>
        </DialogSpecimen>
      </Dialog>
    </div>
  ),
}
