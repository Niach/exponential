// EXP-988 contract: activity folding (owner: EXP-900).
//
// READ-TIME ONLY. `issue_events` rows are never deleted or rewritten; the fold
// runs where the timeline is assembled so every client gets the same result,
// and a `?raw=1` / "show all" path returns the unfolded list. Where that
// assembly happens is EXP-900's call: today the rows reach every client as
// the `issue-events` Electric shape (routes/api/shapes/issue-events.ts), and a
// shape proxy cannot fold, so the fold either runs client-side ×4 over the
// synced rows (this pure function, mirrored) or behind a tRPC feed endpoint
// that replaces the shape read for the timeline.
//
// THE FOLD RULE (open decision 3): a run of events on the SAME issue and the
// SAME field by the SAME actor, with no event from any OTHER actor on that
// issue in between, first-to-last within `FOLD_WINDOW_MS`, collapses to its
// NET effect. A net of "nothing changed" (A → B → A) disappears entirely; a
// net A → C shows as ONE event carrying the first `from` and the last `to`
// (timestamped at the LAST event). `created` events never fold away, and
// neither do `pr_opened` / `pr_merged`.
//
// "Field" is derived from the event type: `status_changed` → status,
// `assignee_changed` → assignee, `priority_changed` → priority,
// `board_moved` → board, `label_added`/`label_removed` → that ONE label
// (`payload.labelId`; an add and a remove of the same label cancel),
// `relation_added`/`relation_removed` → that one relation. Events whose field
// is unknown never fold (`foldFieldKey` returns null).
import type { IssueEvent } from "@/db/schema"

/** The row shape the fold reads: the synced `issue_events` columns. */
export type ActivityEvent = Pick<
  IssueEvent,
  `id` | `issueId` | `actorUserId` | `type` | `payload` | `createdAt`
>

/** First-to-last span a run may cover and still fold. */
export const FOLD_WINDOW_MS = 10 * 60 * 1000

/** The field an event edits, or null for an event that never folds. */
export function foldFieldKey(_event: ActivityEvent): string | null {
  throw new Error(`foldFieldKey is not implemented yet (EXP-900 owns it)`)
}

/** Input in chronological order (oldest first); output likewise. */
export function foldActivity(_events: ActivityEvent[]): ActivityEvent[] {
  throw new Error(`foldActivity is not implemented yet (EXP-900 owns it)`)
}
