import { useCallback, useState } from "react"
import { format, parseISO } from "date-fns"
import { Megaphone, X } from "lucide-react"
import {
  Button,
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
  Separator,
  useIsMobile,
} from "@exp/ui"
import { cn } from "@/lib/utils"
import { MarkdownEditor } from "@/components/issue-editor/markdown-editor"
import {
  CHANGELOG,
  latestChangelogEntry,
  type ChangelogEntry,
} from "@/lib/changelog"
import { markChangelogSeen, readSeenChangelogId } from "@/lib/changelog-seen"

// "What's new" changelog surfaces (EXP-164): a small dismissable card floating
// over the bottom of the sidebar's scroll area previewing the latest release
// note, and the detailed sheet it opens. The card keys on the HEAD entry of
// `CHANGELOG` — dismissing (or opening) stores that entry's id per-device, and
// the card stays hidden until a release prepends a fresh entry.

/** The card's state, owned by the sidebar (EXP-1022): the card FLOATS over
 *  the scroll area, so the scroll content needs to know whether to reserve
 *  room under its last row — visibility has to live one level up. */
export interface WhatsNewState {
  latest: ChangelogEntry | null
  visible: boolean
  dismiss: () => void
}

export function useWhatsNew(): WhatsNewState {
  const latest = latestChangelogEntry()
  // Read once at mount — dismissal updates React state immediately, so the
  // stored value only matters for the initial render.
  const [seenId] = useState(() => readSeenChangelogId())
  const [dismissed, setDismissed] = useState(false)
  const dismiss = useCallback(() => {
    if (!latest) return
    setDismissed(true)
    markChangelogSeen(latest.id)
  }, [latest])
  return {
    latest,
    visible: latest !== null && !dismissed && seenId !== latest.id,
    dismiss,
  }
}

/** EXP-1022: the vertical room the sidebar's scroll content keeps under its
 *  last row while the card floats (its height plus a gap), so the final entry
 *  can still scroll clear of it. The desktop rail reserves the same
 *  (`sidebar.rs` `WHATS_NEW_CLEARANCE`). */
export const WHATS_NEW_CLEARANCE_CLASS = `pb-20`

export function WhatsNewCard({
  state,
  onOpen,
  className,
}: {
  state: WhatsNewState
  onOpen: () => void
  className?: string
}) {
  const { latest, visible, dismiss } = state
  if (!latest || !visible) return null

  return (
    <div
      role="button"
      tabIndex={0}
      aria-label={`What's new: ${latest.title}`}
      className={cn(
        `cursor-pointer rounded-lg border bg-card p-3 shadow-md transition-colors hover:bg-accent/50`,
        className
      )}
      onClick={() => {
        dismiss()
        onOpen()
      }}
      onKeyDown={(event) => {
        // Only activate for keys aimed at the card itself — keydown from the
        // focused Dismiss button bubbles up here, and preventDefault would
        // suppress its native activation and open the sheet instead.
        if (event.target !== event.currentTarget) return
        if (event.key === `Enter` || event.key === ` `) {
          event.preventDefault()
          dismiss()
          onOpen()
        }
      }}
    >
      <div className="flex items-center justify-between gap-2">
        <div className="flex min-w-0 items-center gap-2">
          <Megaphone className="size-4 shrink-0 text-muted-foreground" />
          <span className="text-sm font-medium">What&apos;s new</span>
        </div>
        <Button
          variant="ghost"
          size="sm"
          className="h-6 w-6 shrink-0 p-0 text-muted-foreground"
          aria-label="Dismiss what's new"
          onClick={(event) => {
            event.stopPropagation()
            dismiss()
          }}
        >
          <X className="size-4" />
        </Button>
      </div>
      <p className="mt-1 truncate text-xs text-muted-foreground">
        {latest.summary}
      </p>
    </div>
  )
}

export function ChangelogSheet({
  open,
  onOpenChange,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  // EXP-449: bottom sheet on mobile (a right drawer covers the whole phone
  // viewport anyway and fights the back gesture), side panel on desktop.
  const isMobile = useIsMobile()
  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent
        side={isMobile ? `bottom` : `right`}
        className={
          isMobile
            ? `gap-0 pb-[max(1rem,env(safe-area-inset-bottom))]`
            : `overflow-y-auto sm:max-w-xl`
        }
      >
        <SheetHeader>
          <SheetTitle>What&apos;s new</SheetTitle>
          <SheetDescription>
            Recent releases and improvements.
          </SheetDescription>
        </SheetHeader>
        {/* On mobile the sheet frame stays put and this list scrolls inside
            the shared 90dvh cap (EXP-687); on desktop the panel itself
            scrolls, so the wrapper is a plain block. Keyed on `isMobile`
            (768) like `side`, not on `max-sm:` (640), so the 640-767 band
            that still gets the bottom sheet can scroll it. */}
        <div
          className={
            isMobile
              ? `min-h-0 flex-1 space-y-6 overflow-y-auto px-4 pb-6`
              : `space-y-6 px-4 pb-6`
          }
        >
          {CHANGELOG.map((entry, index) => (
            <div key={entry.id}>
              {index > 0 && <Separator className="mb-6" />}
              <div className="mb-1 text-xs text-muted-foreground">
                {format(parseISO(entry.date), `MMMM d, yyyy`)}
              </div>
              <h3 className="mb-2 text-base font-semibold">{entry.title}</h3>
              <MarkdownEditor
                editable={false}
                markdown={entry.body}
                onChange={() => {}}
              />
            </div>
          ))}
        </div>
      </SheetContent>
    </Sheet>
  )
}
