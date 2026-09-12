import { useState } from "react"
import { conceptIcon } from "@/lib/icons.generated"
import { AddDelCounts } from "@/components/diff-view"
import {
  fileCardMoreLabel,
  fileCardTitle,
  FILE_CARD_PREVIEW,
  type SessionFileCard as SessionFileCardData,
} from "@/lib/session-file-cards"
import { cn } from "@/lib/utils"

// EXP-850 §12: the card that closes a turn segment — what the agent actually
// changed, at the end of the turn that changed it. Clicking a row opens the
// session's diff pane scrolled to that file. The derivation is pure
// (`lib/session-file-cards.ts`); this only draws it.
const CodingDiffIcon = conceptIcon(`coding-diff`)

export function SessionFileCard({
  card,
  onOpenFile,
  className,
}: {
  card: SessionFileCardData
  onOpenFile: (path: string) => void
  className?: string
}) {
  const [expanded, setExpanded] = useState(false)
  const more = fileCardMoreLabel(card.files.length)
  const shown =
    expanded || more === null ? card.files : card.files.slice(0, FILE_CARD_PREVIEW)
  return (
    <div
      className={cn(
        `min-w-0 rounded-md border border-border/60 bg-muted/20`,
        className
      )}
      data-testid="session-file-card"
    >
      <div className="flex min-w-0 items-center gap-2 px-3 py-1.5">
        <CodingDiffIcon className="size-3 shrink-0 text-muted-foreground/60" />
        <span className="min-w-0 truncate font-medium">
          {fileCardTitle(card.files.length)}
        </span>
      </div>
      <div className="pb-1">
        {shown.map((file) => (
          <button
            key={file.path}
            type="button"
            onClick={() => onOpenFile(file.path)}
            className="flex w-full min-w-0 items-center gap-2 px-3 py-0.5 text-left text-muted-foreground hover:text-foreground"
            title={file.path}
          >
            <span className="min-w-0 flex-1 truncate font-mono">{file.path}</span>
            <AddDelCounts
              additions={file.additions}
              deletions={file.deletions}
            />
          </button>
        ))}
        {more && (
          <button
            type="button"
            onClick={() => setExpanded((open) => !open)}
            className="px-3 py-0.5 text-left text-muted-foreground hover:text-foreground"
          >
            {expanded ? `Show less` : more}
          </button>
        )}
      </div>
    </div>
  )
}
