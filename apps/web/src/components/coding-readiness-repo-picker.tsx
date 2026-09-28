import { useMemo, useState } from "react"
import { Button, PickerList, conceptIcon, type PickerItem } from "@exp/ui"
import type { Board } from "@/db/schema"
import { trpc } from "@/lib/trpc-client"
import {
  READINESS_COPY,
  readinessRepoRows,
} from "@/lib/coding-readiness"
import {
  GithubRepoPicker,
  type PickerRepo,
} from "@/components/github-repo-picker"
import type { ReadinessRepoList } from "@/hooks/use-coding-readiness"

// EXP-1121: the "Choose repository" fix, INLINE in the checklist's current
// row (it replaces the fix buttons while open). The team's repositories with
// the board-name match on top ("matches board"), the rest tagged with the
// board already using them ("used by Website"); picking one points the board
// at it (`boards.setRepository`, members hold `mutate_resources`) and the
// row ticks off the synced board shape. The footer row expands the shared
// GitHub picker in place — the Add-repository flow of Settings ›
// Repositories — which connects a brand-new repository and points the board
// at it in one go.

const GithubIcon = conceptIcon(`ui-github`)
const AddIcon = conceptIcon(`ui-add`)

export function ReadinessRepoPicker({
  teamId,
  board,
  repos,
  onPicked,
  onReload,
}: {
  teamId: string
  board: Board
  repos: ReadinessRepoList | null
  /** The board now points at a repository — close the picker. */
  onPicked: () => void
  onReload: () => void
}) {
  const [query, setQuery] = useState(``)
  const [busy, setBusy] = useState(false)
  const [adding, setAdding] = useState(false)
  const rows = useMemo(
    () => (repos ? readinessRepoRows(repos, board, query) : []),
    [repos, board, query]
  )
  const items = useMemo<PickerItem[]>(
    () =>
      rows.map((row) => ({
        value: row.id,
        label: row.fullName,
        icon: GithubIcon,
        keywords: [row.fullName],
      })),
    [rows]
  )
  const byId = useMemo(() => new Map(rows.map((row) => [row.id, row])), [rows])

  const point = async (repositoryId: string) => {
    if (busy) return
    setBusy(true)
    try {
      // Errors toast through the client's link; the row stays as it was.
      await trpc.boards.setRepository.mutate({
        boardId: board.id,
        repositoryId,
      })
      onPicked()
    } catch {
      // Already toasted.
    } finally {
      setBusy(false)
    }
  }

  // A repository the team never had: register it (idempotent), then point
  // the board at it — the board settings page's connect-new path.
  const connectNew = async (picked: PickerRepo) => {
    if (busy) return
    setBusy(true)
    try {
      const { repository } = await trpc.repositories.add.mutate({
        teamId,
        fullName: picked.fullName,
        defaultBranch: picked.defaultBranch,
        private: picked.private,
      })
      if (repository) {
        await trpc.boards.setRepository.mutate({
          boardId: board.id,
          repositoryId: repository.id,
        })
        onReload()
        onPicked()
      }
    } catch {
      // Already toasted.
    } finally {
      setBusy(false)
    }
  }

  if (adding) {
    return (
      <div className="flex flex-col gap-2" data-testid="readiness-repo-add">
        <GithubRepoPicker
          teamId={teamId}
          variant="plain"
          listClassName="max-h-48"
          onSelect={(picked) => void connectNew(picked)}
        />
      </div>
    )
  }

  return (
    <div
      className="overflow-hidden rounded-md border border-glass-stroke-card bg-glass-card"
      data-testid="readiness-repo-picker"
    >
      <PickerList
        mode="single"
        value={null}
        onChange={(repositoryId) => void point(repositoryId)}
        items={items}
        search
        searchPlaceholder={READINESS_COPY.pickerSearch}
        query={query}
        onQueryChange={setQuery}
        shouldFilter={false}
        loading={repos === null}
        emptyText={
          repos !== null && repos.length === 0
            ? READINESS_COPY.pickerEmpty
            : undefined
        }
        listClassName="max-h-48"
        renderItem={(item) => {
          const row = byId.get(item.value)
          return (
            <>
              <GithubIcon aria-hidden className="size-4 shrink-0" />
              <span className="min-w-0 flex-1 truncate text-left text-sm">
                {item.label}
              </span>
              {row?.tag && (
                <span
                  className={
                    row.matches
                      ? `shrink-0 text-xs text-emerald-400`
                      : `shrink-0 text-xs text-muted-foreground`
                  }
                >
                  {row.tag}
                </span>
              )}
            </>
          )
        }}
        footer={
          <Button
            variant="ghost"
            size="sm"
            className="w-full justify-start rounded-none text-muted-foreground"
            disabled={busy}
            onClick={() => setAdding(true)}
          >
            <AddIcon />
            {READINESS_COPY.pickerAddFromGithub}
          </Button>
        }
      />
    </div>
  )
}
