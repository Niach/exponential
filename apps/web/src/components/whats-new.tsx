import { useState } from "react"
import { format, parseISO } from "date-fns"
import { Megaphone, X } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet"
import { Separator } from "@/components/ui/separator"
import { MarkdownEditor } from "@/components/issue-editor/markdown-editor"
import { CHANGELOG, latestChangelogEntry } from "@/lib/changelog"
import { markChangelogSeen, readSeenChangelogId } from "@/lib/changelog-seen"

// "What's new" changelog surfaces (EXP-164): a small dismissable card in the
// sidebar footer previewing the latest release note, and the detailed sheet
// it opens. The card keys on the HEAD entry of `CHANGELOG` — dismissing (or
// opening) stores that entry's id per-device, and the card stays hidden until
// a release prepends a fresh entry.

export function WhatsNewCard({ onOpen }: { onOpen: () => void }) {
  const latest = latestChangelogEntry()
  // Read once at mount — dismissal updates React state immediately, so the
  // stored value only matters for the initial render.
  const [seenId] = useState(() => readSeenChangelogId())
  const [dismissed, setDismissed] = useState(false)

  if (!latest || dismissed || seenId === latest.id) return null

  const dismiss = () => {
    setDismissed(true)
    markChangelogSeen(latest.id)
  }

  return (
    <div
      role="button"
      tabIndex={0}
      aria-label={`What's new: ${latest.title}`}
      className="cursor-pointer rounded-lg border bg-card p-3 shadow-sm transition-colors hover:bg-accent/50"
      onClick={() => {
        dismiss()
        onOpen()
      }}
      onKeyDown={(event) => {
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
  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent side="right" className="overflow-y-auto sm:max-w-xl">
        <SheetHeader>
          <SheetTitle>What&apos;s new</SheetTitle>
          <SheetDescription>
            Recent releases and improvements.
          </SheetDescription>
        </SheetHeader>
        <div className="space-y-6 px-4 pb-6">
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
