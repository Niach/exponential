import { and, isNull } from "drizzle-orm"
import { boards } from "@/db/schema"

/**
 * A board that is neither trashed nor archived — the ONLY kind any read
 * surface, list or mutation resolves.
 *
 * Trash (`deleted_at`, REV2-5) hides a board for 48h and then purges it;
 * archive (`archived_at`, EXP-500) hides it indefinitely and never purges.
 * Both are invisible to every client and every server query, so the two always
 * travel together: wherever this predicate is missing from a boards join, an
 * archived board leaks. The Electric shapes enforce the same rule with their
 * static `board_deleted_at IS NULL AND board_archived_at IS NULL` mirrors.
 *
 * Deliberately a standalone module rather than part of `lib/auth/membership`:
 * it is a pure SQL predicate with no session or database access, and tests
 * that mock the membership barrel must not have to stub it out.
 *
 * The archive/unarchive, delete/restore and listArchived/listDeleted
 * procedures do NOT use it — they must resolve the very rows it hides, so they
 * select on `boards.id` directly.
 */
export function boardVisible() {
  return and(isNull(boards.deletedAt), isNull(boards.archivedAt))
}
