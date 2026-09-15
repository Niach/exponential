import { useState, type ReactNode } from "react"
import type { User } from "@/db/schema"
import {
  conceptIcon,
  Sheet,
  SheetContent,
  SheetHeader,
  SheetTitle,
} from "@exp/ui"
import { CommentComposer } from "@/components/comment-composer"
import {
  MOBILE_WORK_CIRCLE_CLASS,
  MobileWorkBar,
  MobileWorkCapsule,
} from "@/components/mobile-work-bar"

// EXP-568 — the phone issue-detail bottom bar: the three things a reader
// actually reaches for, floating over the thread instead of buried at the
// bottom of a long scroll. Properties (a sheet), Comment (expands into the
// composer in place), and the trailing circle (passed in — EXP-893: the
// Start coding circle while the issue has no run of mine, else the face
// switcher of the Work screen). EXP-893: a composition over the ONE
// `MobileWorkBar` every face of the Work screen shares.

const PropertiesIcon = conceptIcon(`ui-properties`)
const AddIcon = conceptIcon(`ui-add`)

export function IssueDetailMobileBar({
  issueId,
  users,
  propertiesNode,
  trailingNode,
  onSubmitComment,
  hidden = false,
}: {
  issueId: string
  users: User[]
  /** The properties chip row — shown inside the bottom sheet. */
  propertiesNode: ReactNode
  /** The right circle: the Start coding circle or the face switcher, or
   *  null. */
  trailingNode: ReactNode
  onSubmitComment: (body: string, attachmentIds: string[]) => Promise<void>
  /** Hidden while the description editor is focused — the keyboard rail owns
   *  the bottom edge then. */
  hidden?: boolean
}) {
  const [propertiesOpen, setPropertiesOpen] = useState(false)
  const [composing, setComposing] = useState(false)

  // EXP-771: nothing to dodge here any more. The widget's mobile edge tab sat
  // mid-right, exactly where the properties sheet slides in, so this bar hid
  // the launcher for as long as the sheet was up (EXP-642). The in-app widget
  // is headless now — it only opens from the sidebar's Report bug entry — so
  // the corner is ours and the hide/restore dance is gone.

  return (
    <>
      <MobileWorkBar
        hidden={hidden}
        leading={
          <button
            type="button"
            aria-label="Issue properties"
            onClick={() => setPropertiesOpen(true)}
            className={MOBILE_WORK_CIRCLE_CLASS}
          >
            <PropertiesIcon className="size-5" />
          </button>
        }
        capsule={
          <MobileWorkCapsule onClick={() => setComposing(true)}>
            <AddIcon className="size-4 shrink-0" />
            Comment
          </MobileWorkCapsule>
        }
        trailing={trailingNode}
        expanded={
          composing ? (
            <CommentComposer
              autoFocus
              issueId={issueId}
              users={users}
              onSubmit={async (body, attachmentIds) => {
                await onSubmitComment(body, attachmentIds)
                setComposing(false)
              }}
              onEmptyBlur={() => setComposing(false)}
            />
          ) : null
        }
      />
      <Sheet open={propertiesOpen} onOpenChange={setPropertiesOpen}>
        {/* The sheet frame is the shared one (grabber, opaque, 90dvh cap);
            the properties card is the scroll region inside it (EXP-687). */}
        <SheetContent
          data-testid="issue-properties-sheet"
          side="bottom"
          className="gap-0 p-2 pb-[max(1rem,env(safe-area-inset-bottom))]"
        >
          {/* EXP-698 r5: a VISIBLE title, like every other bottom sheet on
              every client (iOS/Android both head this one "Properties") —
              a sheet that slides up unlabelled reads as a menu that lost its
              heading. Same `SheetHeader`/`SheetTitle` pair (and drag area) as
              the board switcher. */}
          <SheetHeader className="px-3 pt-2 pb-1">
            <SheetTitle>Properties</SheetTitle>
          </SheetHeader>
          <div className="min-h-0 flex-1 overflow-y-auto">{propertiesNode}</div>
        </SheetContent>
      </Sheet>
    </>
  )
}
