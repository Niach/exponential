import { Link } from "@tanstack/react-router"

// Dead end for a board slug that matches no synced board (REV2-59): trashed
// boards drop out of the boards shape and slug renames orphan old links, so a
// stale bookmark has to say so instead of spinning forever. `/t/$teamSlug`
// auto-picks a live board.
export function BoardNotFound({
  boardSlug,
  teamSlug,
}: {
  boardSlug: string
  teamSlug: string
}) {
  return (
    <div className="flex flex-col items-start gap-3 p-6 text-sm">
      <div className="text-muted-foreground">
        Board <span className="font-mono">{boardSlug}</span> not found — it may
        have been deleted or renamed.
      </div>
      <Link
        to="/t/$teamSlug"
        params={{ teamSlug }}
        className="text-foreground underline-offset-2 hover:underline"
      >
        ← Back to team
      </Link>
    </div>
  )
}
