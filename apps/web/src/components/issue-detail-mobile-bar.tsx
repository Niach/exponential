import { useState, type ReactNode } from "react"
import type { User } from "@/db/schema"
import { conceptIcon } from "@/lib/icons.generated"
import { CommentComposer } from "@/components/comment-composer"
import { Sheet, SheetContent, SheetTitle } from "@/components/ui/sheet"
import { cn } from "@/lib/utils"

// EXP-568 — the phone issue-detail bottom bar: the three things a reader
// actually reaches for, floating over the thread instead of buried at the
// bottom of a long scroll. Properties (a sheet), Comment (expands into the
// composer in place), and the coding circle (passed in — its gating lives in
// issue-coding-rows.tsx, which is where `useRemoteStart` belongs).
//
// The glass recipe is the mobile tab bar's, so the two bars read as one system
// when both are on screen.

const PropertiesIcon = conceptIcon(`ui-properties`)
const AddIcon = conceptIcon(`ui-add`)

const CIRCLE_CLASS = `pointer-events-auto flex size-[52px] shrink-0 items-center justify-center rounded-full border border-glass-stroke-card bg-popover/85 text-muted-foreground shadow-lg shadow-black/40 backdrop-blur-xl`

export function IssueDetailMobileBar({
  issueId,
  users,
  propertiesNode,
  codingNode,
  onSubmitComment,
  hidden = false,
}: {
  issueId: string
  users: User[]
  /** The properties chip row — shown inside the bottom sheet. */
  propertiesNode: ReactNode
  /** The coding circle (IssueCodingControl variant="fab"), or null. */
  codingNode: ReactNode
  onSubmitComment: (body: string, attachmentIds: string[]) => Promise<void>
  /** Hidden while the description editor is focused — the keyboard rail owns
   *  the bottom edge then. */
  hidden?: boolean
}) {
  const [propertiesOpen, setPropertiesOpen] = useState(false)
  const [composing, setComposing] = useState(false)

  if (hidden) return null

  return (
    <>
      <div className="pointer-events-none fixed inset-x-0 bottom-0 z-[35] flex items-end gap-2 px-4 pb-[max(1rem,env(safe-area-inset-bottom))] md:hidden">
        {composing ? (
          <div className="pointer-events-auto w-full animate-in fade-in zoom-in-95 duration-fast ease-standard motion-reduce:animate-none">
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
          </div>
        ) : (
          <>
            <button
              type="button"
              aria-label="Issue properties"
              onClick={() => setPropertiesOpen(true)}
              className={CIRCLE_CLASS}
            >
              <PropertiesIcon className="size-5" />
            </button>
            <button
              type="button"
              onClick={() => setComposing(true)}
              className={cn(
                CIRCLE_CLASS,
                `h-[52px] w-auto flex-1 justify-start gap-2 px-4 text-sm`
              )}
            >
              <AddIcon className="size-4 shrink-0" />
              Comment
            </button>
          </>
        )}
        {/* Kept MOUNTED across the compose swap — the coding circle owns a
            device lookup that must not re-run on every expand. `hidden` beats
            `contents` through tailwind-merge, so this is display:none while
            composing and a transparent wrapper otherwise. */}
        <div className={cn(`contents`, composing && `hidden`)}>
          {codingNode}
        </div>
      </div>
      <Sheet open={propertiesOpen} onOpenChange={setPropertiesOpen}>
        <SheetContent side="bottom" className="max-h-[80dvh] overflow-y-auto p-2 pb-[max(1rem,env(safe-area-inset-bottom))]">
          <SheetTitle className="sr-only">Issue properties</SheetTitle>
          {propertiesNode}
        </SheetContent>
      </Sheet>
    </>
  )
}
