import type { ReactNode } from "react"
import {
  SEGMENTED_TAB,
  Tabs,
  TabsList,
  TabsTrigger,
} from "@/components/ui/tabs"
import type { WorkTabFace } from "@/lib/work-tabs"

// EXP-870: an issue and its run are ONE work tab with faces. This is the
// segmented control the unified work header carries (desktop `work_header.rs`
// top row) — the same segmented pill as the list nav's Inbox / My Issues
// strip. EXP-877: the faces are `Issue` (issue-bound), `Run` (a run exists)
// and the diff (`+N -M`, once the run has changes); an unavailable face is
// HIDDEN, never disabled, and the control itself is absent under two faces.

/** The three faces a work tab can show. `diff` is the run's changes. */
export type WorkFace = WorkTabFace | `diff`

export interface WorkFaceItem {
  face: WorkFace
  label: ReactNode
  onSelect: () => void
}

/** Byte-identical with the IDE. */
export const ISSUE_FACE_LABEL = `Issue`
export const RUN_FACE_LABEL = `Run`

/** The diff face's label: `+N -M` in mono, the ASCII minus, the diff pill's
 * own green/red. No glyph. */
export function DiffFaceLabel({
  additions,
  deletions,
}: {
  additions: number
  deletions: number
}) {
  return (
    <span className="font-mono">
      <span className="text-emerald-400">+{additions}</span>
      {` `}
      <span className="text-rose-400">-{deletions}</span>
    </span>
  )
}

export function WorkFaceToggle({
  face,
  items,
}: {
  face: WorkFace
  items: readonly WorkFaceItem[]
}) {
  if (items.length < 2) return null
  return (
    <Tabs
      value={face}
      onValueChange={(next) => {
        if (next === face) return
        items.find((item) => item.face === next)?.onSelect()
      }}
      className="w-fit shrink-0"
      data-testid="work-face-toggle"
    >
      <TabsList>
        {items.map((item) => (
          <TabsTrigger
            key={item.face}
            value={item.face}
            className={SEGMENTED_TAB}
            data-face={item.face}
          >
            {item.label}
          </TabsTrigger>
        ))}
      </TabsList>
    </Tabs>
  )
}
