import { useState, useMemo, useEffect } from "react"
import { useNavigate } from "@tanstack/react-router"
import { useLiveQuery, inArray } from "@tanstack/react-db"
import { Sheet, SheetContent, SheetTitle } from "@/components/ui/sheet"
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
  Command,
  CommandEmpty,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command"
import { issueCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import { useTeamBoards } from "@/hooks/use-team-data"
import { useIsMobile } from "@/hooks/use-mobile"
import { StatusIcon } from "@/components/issue-properties/status-dropdown"
import { Search } from "lucide-react"
import type { Issue, Board } from "@/db/schema"

interface IssueSearchSheetProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  teamId: string
  teamSlug: string
}

// The minimal fields a result row needs to render + navigate. Local Electric
// `Issue` rows satisfy this structurally; server FTS hits (issues.search)
// provide exactly these fields.
interface SearchResult {
  id: string
  identifier: string
  title: string
  boardId: string
  status: string
}

type ServerHit = Awaited<ReturnType<typeof trpc.issues.search.query>>[number]

// One search experience, two presentations: a full-screen bottom sheet on
// mobile (reached from the topbar) and a centered cmdk dialog on desktop
// (reached from the sidebar or Cmd/Ctrl+F). The search logic is shared; the
// desktop container is a `Command` so keyboard users get arrow-key row
// selection and Enter-to-open for free, while mobile stays touch-only.
export function IssueSearchSheet({
  open,
  onOpenChange,
  teamId,
  teamSlug,
}: IssueSearchSheetProps) {
  const [query, setQuery] = useState(``)
  const navigate = useNavigate()
  const isMobile = useIsMobile()
  const boards = useTeamBoards(teamId)
  const boardIds = useMemo(() => boards.map((p: Board) => p.id), [boards])
  const boardMap = useMemo(
    () => new Map<string, Board>(boards.map((p: Board) => [p.id, p])),
    [boards]
  )

  const { data: issues } = useLiveQuery(
    (q) =>
      boardIds.length > 0
        ? q
            .from({ issues: issueCollection })
            .where(({ issues }) => inArray(issues.boardId, boardIds))
        : undefined,
    [boardIds.join(`,`)]
  )

  // Server full-text search ("search everything" path): debounced ~250ms,
  // additive on top of the instant local substring filter. Hits are keyed by
  // the query they answered so a stale response for an earlier keystroke
  // never leaks into the current result list. Errors are swallowed — the
  // search box must never block on the network.
  const [serverHits, setServerHits] = useState<{
    query: string
    rows: ServerHit[]
  }>({ query: ``, rows: [] })

  useEffect(() => {
    const trimmed = query.trim()
    if (trimmed === ``) return
    let cancelled = false
    const timer = setTimeout(() => {
      trpc.issues.search
        .query({ teamId, query: trimmed, limit: 30 })
        .then((rows) => {
          if (!cancelled) setServerHits({ query: trimmed, rows })
        })
        .catch(() => {
          // Fall back to local-only results on server/network errors.
        })
    }, 250)
    return () => {
      cancelled = true
      clearTimeout(timer)
    }
  }, [query, teamId])

  const localById = useMemo(
    () => new Map<string, Issue>((issues ?? []).map((i: Issue) => [i.id, i])),
    [issues]
  )

  const results = useMemo(() => {
    const q = query.trim()
    if (!q) return [] as SearchResult[]
    const lower = q.toLowerCase()
    // Fast path: instant local title-substring matches.
    const local = (issues ?? []).filter((i: Issue) =>
      i.title.toLowerCase().includes(lower)
    )
    const merged: SearchResult[] = [...local]
    const seen = new Set(local.map((i: Issue) => i.id))
    // Merge server FTS hits (deduped by id) once they answer the CURRENT
    // query. Prefer the local Electric row when the id is synced locally so
    // rows render identically; otherwise render from the server fields.
    if (serverHits.query === q) {
      for (const hit of serverHits.rows) {
        if (seen.has(hit.id)) continue
        seen.add(hit.id)
        merged.push(localById.get(hit.id) ?? hit)
      }
    }
    return merged.slice(0, 30)
  }, [issues, query, serverHits, localById])

  const handleOpenChange = (o: boolean) => {
    onOpenChange(o)
    if (!o) setQuery(``)
  }

  const handlePick = (issue: SearchResult) => {
    const board = boardMap.get(issue.boardId)
    if (!board) return
    onOpenChange(false)
    setQuery(``)
    void navigate({
      to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
      params: {
        teamSlug,
        boardSlug: board.slug,
        issueIdentifier: issue.identifier,
      },
    })
  }

  const emptyState =
    query.trim() === `` ? (
      <div className="flex flex-col items-center justify-center p-12 text-muted-foreground">
        <Search className="size-8 mb-3 opacity-50" />
        <p className="text-sm">Type to search issues</p>
      </div>
    ) : (
      <div className="flex flex-col items-center justify-center p-12 text-muted-foreground">
        <p className="text-sm">No issues match "{query}"</p>
      </div>
    )

  const resultRow = (issue: SearchResult) => {
    const board = boardMap.get(issue.boardId)
    return (
      <>
        <StatusIcon status={issue.status} className="size-4 shrink-0" />
        <div className="flex flex-col flex-1 min-w-0">
          <span className="text-sm truncate">{issue.title}</span>
          {board && (
            <span className="flex items-center gap-1.5 text-xs text-muted-foreground">
              <span
                className="h-1.5 w-1.5 rounded-full shrink-0"
                style={{ backgroundColor: board.color }}
              />
              <span className="truncate">
                {board.name} · {issue.identifier}
              </span>
            </span>
          )}
        </div>
      </>
    )
  }

  if (isMobile) {
    return (
      <Sheet open={open} onOpenChange={handleOpenChange}>
        <SheetContent
          side="bottom"
          showCloseButton={false}
          className="top-0 h-[100dvh] p-0 gap-0 flex flex-col"
        >
          <SheetTitle className="sr-only">Search issues</SheetTitle>
          <div className="flex items-center gap-2 px-3 py-3 border-b border-border/50">
            <Search className="size-4 text-muted-foreground shrink-0" />
            <Input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Search issues..."
              autoFocus
              className="border-none shadow-none focus-visible:ring-0 h-9 text-base md:text-sm"
            />
            <Button
              type="button"
              variant="ghost"
              size="sm"
              onClick={() => handleOpenChange(false)}
              className="text-sm text-muted-foreground px-2"
            >
              Cancel
            </Button>
          </div>
          <div className="flex-1 overflow-y-auto">
            {results.length === 0 && emptyState}
            {results.map((issue) => (
              <Button
                key={issue.id}
                type="button"
                variant="ghost"
                onClick={() => handlePick(issue)}
                className="flex h-auto w-full items-center justify-start gap-3 rounded-none px-4 py-3 text-left font-normal hover:bg-accent active:bg-accent/70 border-b border-border/30"
              >
                {resultRow(issue)}
              </Button>
            ))}
          </div>
        </SheetContent>
      </Sheet>
    )
  }

  // Desktop: cmdk owns the keyboard model (ArrowUp/Down move the highlighted
  // row, Enter opens it, the first result is pre-selected as results arrive).
  // Its internal filtering is off — `results` is already the local+server
  // merge — so items render exactly as computed.
  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent
        showCloseButton={false}
        className="p-0 gap-0 flex flex-col overflow-hidden sm:top-[15%] sm:max-h-[60vh] sm:translate-y-0 sm:max-w-lg"
      >
        <DialogTitle className="sr-only">Search issues</DialogTitle>
        <Command
          shouldFilter={false}
          className="min-h-0 bg-transparent **:data-[slot=command-input-wrapper]:h-14 **:data-[slot=command-input-wrapper]:border-border/50"
        >
          <CommandInput
            value={query}
            onValueChange={setQuery}
            placeholder="Search issues..."
            autoFocus
            className="text-base md:text-sm"
          />
          <CommandList className="max-h-none flex-1 overflow-y-auto">
            <CommandEmpty className="p-0">{emptyState}</CommandEmpty>
            {results.map((issue) => (
              <CommandItem
                key={issue.id}
                value={issue.id}
                onSelect={() => handlePick(issue)}
                className="gap-3 rounded-none px-4 py-3 cursor-pointer border-b border-border/30"
              >
                {resultRow(issue)}
              </CommandItem>
            ))}
          </CommandList>
        </Command>
      </DialogContent>
    </Dialog>
  )
}
