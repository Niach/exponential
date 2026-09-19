import { normalizeIssueDescriptionText } from "@/lib/domain"

/** EXP-928 — field saves whose Electric echo has not landed yet.
 *
 * A save is a tRPC round trip and the synced row only catches up with the
 * echo. Until then the collection still holds the text the save REPLACED, and
 * every reader that trusted it undid the save locally: an issue left and
 * reopened right after a blur save (the view is reused across issue switches,
 * only the `[issue.id]` effect reseeds it) came back with its OLD description,
 * and typing into that stale buffer made the next blur write old text + the
 * new typing over the real save.
 *
 * Per issue: the text last saved, plus every earlier value the row may still
 * show while saves are in flight (the pre-save row, the pre-save baseline and
 * older saves whose echoes are queued ahead of this one). A row showing one of
 * those is stale; the saved text itself is the echo, and anything else is a
 * newer remote write — both retire the entry.
 *
 * The desktop twin is `crates/ui/src/issue_detail.rs` `UnechoedSaves`; the two
 * keep the same semantics on purpose.
 */

/** How long a save may stay un-echoed before the row wins again. Generous
 *  against a slow round trip, short enough that a lost one heals within a
 *  reading pause. */
export const UNECHOED_SAVE_TTL_MS = 60_000

interface UnechoedSave {
  saved: string
  /** Values the row may still be showing, oldest first. */
  stale: string[]
  /** When the newest save in this entry was sent. An entry nobody retired
   *  within the TTL is bookkeeping, not an in-flight write (a dropped
   *  request, a killed tab, a server that answered nothing at all), and
   *  keeping it would hold the editor on text the row does not have. */
  at: number
}

export class UnechoedSaves {
  private entries = new Map<string, UnechoedSave>()

  /** Record that `saved` (normalized) was just sent for `id`, while the row
   *  and the view's baselines still read `stale` (normalized too). */
  record(
    id: string,
    saved: string,
    stale: readonly string[],
    now: number = Date.now()
  ) {
    const staleValues = Array.from(new Set(stale))
    const entry = this.entries.get(id)
    if (entry) {
      // The same save sent again (a retry, a second blur with no edit in
      // between) — nothing new to track, just keep the entry alive.
      if (entry.saved === saved) {
        entry.at = now
        return
      }
      // A NEWER save on top of a tracked one: the previous saved text joins
      // the values the row may still show, since its echo is queued ahead.
      const tracked = staleValues.some(
        (value) => value === entry.saved || entry.stale.includes(value)
      )
      if (tracked) {
        entry.stale.push(entry.saved)
        for (const value of staleValues) {
          if (value !== saved && !entry.stale.includes(value)) {
            entry.stale.push(value)
          }
        }
        entry.stale = entry.stale.filter((value) => value !== saved)
        entry.saved = saved
        entry.at = now
        return
      }
    }
    // A save that changes nothing the row does not already show needs no
    // tracking at all.
    const pending = staleValues.filter((value) => value !== saved)
    if (pending.length === 0) {
      this.entries.delete(id)
      return
    }
    this.entries.set(id, { saved, stale: pending, at: now })
  }

  /** The text `id` really holds given its synced row (`synced`, normalized):
   *  the in-flight save while the row is stale, else `null` (the row is the
   *  truth) — retiring the entry once the echo or a newer remote write shows
   *  up. */
  resolve(id: string, synced: string, now: number = Date.now()): string | null {
    const entry = this.entries.get(id)
    if (!entry) return null
    if (now - entry.at >= UNECHOED_SAVE_TTL_MS) {
      this.entries.delete(id)
      return null
    }
    const index = entry.stale.indexOf(synced)
    if (index >= 0) {
      // Echoes arrive in save order: values queued before this one are behind
      // the row now, so the row can never go back to them.
      entry.stale = entry.stale.slice(index)
      return entry.saved
    }
    this.entries.delete(id)
    return null
  }

  /** The write of `saved` FAILED: there is no echo coming, so the entry must
   *  go or the editor keeps showing text the server rejected (and every
   *  reopen of the issue restores it). A LATER save for the same issue
   *  supersedes this one and stays. */
  retire(id: string, saved: string) {
    if (this.entries.get(id)?.saved === saved) this.entries.delete(id)
  }
}

/** The ONE description store: `IssueDetailView` is reused across issue
 *  switches and remounts, so the bookkeeping has to outlive both. */
export const descriptionSaves = new UnechoedSaves()

/** The synced description a view should treat as the truth: the un-echoed
 *  save while the row still reads what it replaced, else the row itself
 *  (which also retires the entry — the echo landed, or somebody else wrote
 *  something newer). Normalized like every save, so the caller can compare it
 *  against its own baselines directly. */
export function resolveIncomingDescription(
  saves: UnechoedSaves,
  issueId: string,
  incoming: string,
  now: number = Date.now()
): string {
  const normalized = normalizeIssueDescriptionText(incoming)
  const saved = saves.resolve(issueId, normalized, now)
  return saved ?? incoming
}
