import { useState, type ReactNode } from "react"
import { useNavigate } from "@tanstack/react-router"
import { Prompt } from "@exp/ui"
import type { Board } from "@/db/schema"
import { promptActions, WEB_PROMPTS } from "@/lib/prompts"
import { trpc } from "@/lib/trpc-client"

/** #EXP-2124 board Archive (a board's row menu, its page's `…`): the board
 *  settings' archive behind its confirm. `onScreen` = leave that board's page. */
export function useBoardArchive(onScreen: (board: Board) => boolean): { ask: (board: Board) => void; dialog: ReactNode } {
  const navigate = useNavigate()
  const [archiving, setArchiving] = useState<Board | null>(null)
  const [busy, setBusy] = useState(false)
  const archive = async (board: Board) => {
    setBusy(true)
    try {
      await trpc.boards.archive.mutate({ boardId: board.id })
      setArchiving(null)
      if (onScreen(board)) void navigate({ to: `/home` })
    } finally {
      setBusy(false)
    }
  }
  const copy = archiving ? WEB_PROMPTS.archiveBoard(archiving.name) : null
  return {
    ask: setArchiving,
    dialog:
      copy && archiving ? (
        <Prompt
          open
          onOpenChange={(next) => {
            if (!next && !busy) setArchiving(null)
          }}
          busy={busy}
          title={copy.title}
          body={copy.body}
          data-testid="sidebar-board-archive-confirm"
          actions={promptActions(copy, {
            archive: { busy, onSelect: () => void archive(archiving) },
          })}
        />
      ) : null,
  }
}
