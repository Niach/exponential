import { useEffect, useState } from "react"
import type { ReactNode } from "react"
import { Check, Github, Loader2, Lock, Plus } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { Button } from "@/components/ui/button"
import {
  GithubRepoPicker,
  type PickerRepo,
} from "@/components/github-repo-picker"

type RepoList = Awaited<ReturnType<typeof trpc.repositories.list.query>>
export type ConnectedRepo = RepoList[number]

// The connected-repo list + optional "Connect another repo…" inline-connect
// expansion, shared by the create-project dialog and the workspace "Change
// repository" dialog. Owns loading the workspace's connected repos (registry)
// and re-detecting on window focus so a repo installed through the picker's
// popup shows up when the user returns.
//
// Selecting a connected repo calls `onSelectRegistry`. When `onConnectNew` is
// provided the "Connect another repo…" affordance is shown (and, when no repos
// are connected yet, the inline GithubRepoPicker is rendered directly); picking
// a brand-new repo through it calls `onConnectNew`. Callers own what happens on
// select/connect (immediate mutate vs. deferred selection) and any chosen-row
// display outside the registry list (via `appendedRow`).
export function ConnectedRepoPicker({
  workspaceId,
  value,
  onSelectRegistry,
  onConnectNew,
  installEmptyState,
  appendedRow,
  disabled,
}: {
  workspaceId: string
  // Selected registry repo id (highlight + check), or null.
  value: string | null
  onSelectRegistry: (repo: ConnectedRepo) => void
  onConnectNew?: (repo: PickerRepo) => void
  // App-absent CTA rendered inside the picker (e.g. a link to Repositories
  // settings) instead of the picker's default inline connect prompt.
  installEmptyState?: ReactNode
  // Extra selected row appended below the registry list (e.g. a brand-new repo
  // picked inline but not yet connected).
  appendedRow?: ReactNode
  disabled?: boolean
}) {
  const [repos, setRepos] = useState<RepoList | null>(null)
  const [pickerOpen, setPickerOpen] = useState(false)

  useEffect(() => {
    let active = true
    const load = async () => {
      try {
        const list = await trpc.repositories.list.query({ workspaceId })
        if (active) setRepos(list)
      } catch {
        if (active) setRepos([])
      }
    }
    void load()
    const onFocus = () => void load()
    window.addEventListener(`focus`, onFocus)
    return () => {
      active = false
      window.removeEventListener(`focus`, onFocus)
    }
  }, [workspaceId])

  const handlePickerSelect = (repo: PickerRepo) => {
    onConnectNew?.(repo)
    setPickerOpen(false)
  }

  if (repos === null) {
    return (
      <div className="flex items-center gap-2 rounded-md border px-3 py-6 text-sm text-muted-foreground">
        <Loader2 className="h-4 w-4 animate-spin" />
        Loading repositories…
      </div>
    )
  }

  // No connected repos yet: connect one directly (or, without a connect
  // handler, say so).
  if (repos.length === 0) {
    if (onConnectNew)
      return (
        <GithubRepoPicker
          onSelect={handlePickerSelect}
          installEmptyState={installEmptyState}
        />
      )
    return (
      <div className="rounded-md border px-3 py-2 text-sm text-muted-foreground">
        No repositories connected yet.
      </div>
    )
  }

  return (
    <div className="space-y-2">
      <div className="divide-y rounded-md border">
        {repos.map((repo) => (
          <Button
            key={repo.id}
            type="button"
            variant="ghost"
            disabled={disabled}
            onClick={() => onSelectRegistry(repo)}
            className="flex h-auto w-full items-center justify-start gap-2 rounded-none px-3 py-2 text-left text-sm font-normal"
          >
            <Github className="h-4 w-4 shrink-0 text-muted-foreground" />
            <span className="truncate">{repo.fullName}</span>
            {repo.private && (
              <Lock className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
            )}
            {value === repo.id && (
              <Check className="ml-auto h-4 w-4 shrink-0 text-primary" />
            )}
          </Button>
        ))}
        {appendedRow}
      </div>
      {onConnectNew &&
        (pickerOpen ? (
          <GithubRepoPicker
            onSelect={handlePickerSelect}
            installEmptyState={installEmptyState}
          />
        ) : (
          <Button
            type="button"
            variant="link"
            size="sm"
            className="px-0 text-xs"
            disabled={disabled}
            onClick={() => setPickerOpen(true)}
          >
            <Plus className="mr-1 h-3.5 w-3.5" />
            Connect another repo…
          </Button>
        ))}
    </div>
  )
}
