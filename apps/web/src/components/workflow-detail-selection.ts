// EXP-1084: the workflow page's picker glue — the selection model itself is
// `lib/workflow-selection.ts` (×4, fixture-locked); this file adds the faces,
// the key rules and the body routing the web page needs.

export type WorkflowFace = `issue` | `runs` | `changes` | `results`

export const WORKFLOW_FACES: readonly WorkflowFace[] = [
  `issue`,
  `runs`,
  `changes`,
  `results`,
]

export type {
  StripSelection,
  SelectModifiers,
} from "@/lib/workflow-selection"
export {
  ALL_SELECTION,
  selectNode,
  stepSelection,
  pruneSelection,
} from "@/lib/workflow-selection"

/** The key a keydown steps with, or null. Modified keys (shift included:
 *  shift-arrow selects text / extends elsewhere) never step. */
export function stripStepKey(event: {
  key: string
  metaKey?: boolean
  ctrlKey?: boolean
  altKey?: boolean
  shiftKey?: boolean
}): 1 | -1 | null {
  if (event.metaKey || event.ctrlKey || event.altKey || event.shiftKey) return null
  if (event.key === `ArrowRight` || event.key === `j`) return 1
  if (event.key === `ArrowLeft` || event.key === `k`) return -1
  return null
}

/**
 * What the body draws for a selection × face:
 * - `issue-list`: the picked (or every) issue as the nested list — `All`, or
 *   several nodes.
 * - `issue-detail`: exactly one node's issue, the Work screen's Issue face.
 * - `runs` / `changes` / `results`: the same body for `All` and a pick, the
 *   pick only filters it.
 */
export type WorkflowBody =
  | `issue-list`
  | `issue-detail`
  | `runs`
  | `changes`
  | `results`

export function workflowBody(face: WorkflowFace, selected: number): WorkflowBody {
  if (face === `issue`) return selected === 1 ? `issue-detail` : `issue-list`
  return face
}

/** The letter keys a DOCUMENT-level listener may step with: letters never
 *  scroll, so j/k work from anywhere that is not typing. ←/→ step only while
 *  focus is inside the strip (they scroll the diff and the transcript). */
export function isLetterStepKey(key: string): boolean {
  return key === `j` || key === `k`
}

export function parseWorkflowFace(value: unknown): WorkflowFace | undefined {
  return WORKFLOW_FACES.includes(value as WorkflowFace)
    ? (value as WorkflowFace)
    : undefined
}
