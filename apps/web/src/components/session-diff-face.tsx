import { FileDiffList, FileNav } from "@/components/diff-view"
import type { PullFile } from "@/components/diff-view"
import { Pill } from "@/components/ui/pill"
import { useIsMobile } from "@/hooks/use-mobile"
import { DIFF_SCOPE_ALL_LABEL } from "@/lib/session-file-cards"

// EXP-877: the run's changes as a FACE of the work tab — the full 896px
// column under the same header, the file list over the patches — instead of
// EXP-850 §11's resizable pane beside the transcript (the split, its drag
// handle and the width memory are gone). The reader picks a file in the list
// and the patch list scrolls to it; a file card row in the transcript opens
// this face already scrolled to its file and SCOPED to that turn (EXP-862),
// and the chip widens it back to the whole run.
/** EXP-862: whether a turn-scoped face may widen BACK to the whole run —
 * only once the run has published a session diff. Until then the turn's
 * files are everything there is, and offering the chip would blank the face
 * on click. */
export function canWidenDiffScope(sessionFileCount: number): boolean {
  return sessionFileCount > 0
}

export function SessionDiffFace({
  files,
  selected,
  onSelect,
  scopeLabel = null,
  onClearScope,
}: {
  files: PullFile[]
  /** The file the patch list is scrolled to; null = the top. */
  selected: string | null
  onSelect: (filename: string) => void
  /** EXP-862: ONE turn's files, not the whole run — the chip says which
   *  (`This turn: 3 files`) and clicking it returns to everything. Absent =
   *  run scope, no chip. */
  scopeLabel?: string | null
  onClearScope?: () => void
}) {
  const isMobile = useIsMobile()
  return (
    <div className="flex flex-col gap-3" data-testid="session-diff-face">
      {scopeLabel && onClearScope && (
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <span>{scopeLabel}</span>
          <Pill
            size="sm"
            mode="action"
            className="shrink-0"
            onClick={onClearScope}
            aria-label={DIFF_SCOPE_ALL_LABEL}
            title={DIFF_SCOPE_ALL_LABEL}
            data-testid="session-diff-scope"
          >
            {DIFF_SCOPE_ALL_LABEL}
          </Pill>
        </div>
      )}
      <FileNav
        files={files}
        onJump={onSelect}
        isMobile={isMobile}
        selected={selected}
      />
      <FileDiffList files={files} showFileNav={false} focusFile={selected} />
    </div>
  )
}
