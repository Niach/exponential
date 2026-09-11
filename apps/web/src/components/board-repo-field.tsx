import { useCallback, useEffect, useRef, useState } from "react"
import { Github, Lock, Plus } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { BOARD_REPO_NOTE } from "@/lib/board-copy"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectSeparator,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { BranchCombobox } from "@/components/branch-combobox"
import {
  GithubRepoPicker,
  type PickerRepo,
} from "@/components/github-repo-picker"

type RepoList = Awaited<ReturnType<typeof trpc.repositories.list.query>>
export type ConnectedRepo = RepoList[number]

const NONE = `none`
const CONNECT = `connect`
const INLINE = `inline`

// The board form's repository + branch block (EXP-712), shared by the
// create-board dialog and the per-board settings dialog. Behaves like ONE
// select: "No repository", the team's connected repos, and a trailing
// "Connect another repository…" action that expands the GitHub picker
// underneath (a brand-new repo is reported through `onConnectNew`; the host
// decides whether that connects immediately or waits for submit). Below it,
// only once a repo is chosen, the branch coding sessions start from — the
// repo's default unless the board pins another. Nothing here mutates: the
// host owns persistence (create saves on submit, settings mutates per change).
export function BoardRepoField({
  teamId,
  repositoryId,
  inlineRepo,
  onSelectRegistry,
  onConnectNew,
  branch,
  onBranchChange,
  disabled,
  error,
}: {
  teamId: string
  // Selected registry repo, or null for "No repository".
  repositoryId: string | null
  // A repo picked through the GitHub picker but not connected yet (the create
  // dialog connects it on submit). When set it IS the selection.
  inlineRepo?: PickerRepo | null
  onSelectRegistry: (repo: ConnectedRepo | null) => void
  onConnectNew: (repo: PickerRepo) => void
  // The board's own branch pin; null = the repo's default.
  branch: string | null
  onBranchChange: (branch: string | null) => void
  disabled?: boolean
  error?: string | null
}) {
  const [repos, setRepos] = useState<RepoList | null>(null)
  // FEED-32: a failed list used to collapse to `[]` silently, which rendered
  // the selected repo as a BLANK trigger. Keep the error and say so.
  const [loadError, setLoadError] = useState<string | null>(null)
  const [pickerOpen, setPickerOpen] = useState(false)

  // A late response for a previous team must not overwrite the current list.
  const teamRef = useRef(teamId)
  teamRef.current = teamId

  const reload = useCallback(async () => {
    const forTeam = teamId
    try {
      const list = await trpc.repositories.list.query({ teamId: forTeam })
      if (teamRef.current !== forTeam) return
      setRepos(list)
      setLoadError(null)
    } catch (err) {
      if (teamRef.current !== forTeam) return
      setLoadError(err instanceof Error ? err.message : String(err))
      setRepos((prev) => prev ?? [])
    }
  }, [teamId])

  useEffect(() => {
    setRepos(null)
    setLoadError(null)
    void reload()
    // Re-detect on focus so a repo installed through the picker's popup shows
    // up when the user returns.
    const onFocus = () => void reload()
    window.addEventListener(`focus`, onFocus)
    return () => window.removeEventListener(`focus`, onFocus)
  }, [reload])

  const selectedRepo =
    repositoryId && repos ? repos.find((r) => r.id === repositoryId) : null

  // FEED-32: the host can point the board at a repo this list has never seen
  // — the settings dialog connects a new repo and the LIVE board row flips
  // `repositoryId` before this copy of `repositories.list` is refreshed, so
  // the Select held a value with no matching item and the trigger went blank.
  // Re-list ONCE per unknown id (a genuinely unknown id — an archived repo —
  // must not loop) and label the trigger explicitly meanwhile.
  const reloadedForId = useRef<string | null>(null)
  const [reloadingForId, setReloadingForId] = useState<string | null>(null)
  useEffect(() => {
    if (!repositoryId || !repos || selectedRepo) return
    if (reloadedForId.current === repositoryId) return
    reloadedForId.current = repositoryId
    setReloadingForId(repositoryId)
    void reload().finally(() =>
      setReloadingForId((cur) => (cur === repositoryId ? null : cur))
    )
  }, [repositoryId, repos, selectedRepo, reload])
  const value = inlineRepo
    ? INLINE
    : repositoryId
      ? repositoryId
      : NONE
  const loading = repos === null
  const hasRepos = (repos?.length ?? 0) > 0 || Boolean(inlineRepo)

  // The trigger's label, rendered EXPLICITLY (Radix's SelectValue otherwise
  // echoes the matching item's text — and shows nothing when no item matches
  // the value). Never blank: an unknown id reads "Loading repository…" while
  // the one-shot re-list above is in flight and "Repository unavailable" once
  // it came back without the id.
  const labelRepo = inlineRepo ?? selectedRepo ?? null
  const triggerLabel = labelRepo
    ? labelRepo.fullName
    : !repositoryId
      ? loading
        ? `Loading…`
        : `No repository`
      : loading || reloadingForId === repositoryId
        ? `Loading repository…`
        : `Repository unavailable`

  // The branch that means "follow the repo": `repositories.list` already
  // folds the team's pin into `defaultBranch`; a not-yet-connected repo only
  // knows GitHub's default.
  const repoDefault = inlineRepo
    ? inlineRepo.defaultBranch
    : (selectedRepo?.defaultBranch ?? null)

  return (
    <div className="space-y-4">
      <div className="space-y-2">
        <Label htmlFor="board-repository">Repository</Label>
        <Select
          value={value}
          disabled={disabled || loading}
          onValueChange={(next) => {
            if (next === CONNECT) {
              setPickerOpen(true)
              return
            }
            setPickerOpen(false)
            if (next === NONE) {
              onSelectRegistry(null)
              return
            }
            if (next === INLINE) return
            const repo = repos?.find((r) => r.id === next)
            if (repo) onSelectRegistry(repo)
          }}
        >
          <SelectTrigger id="board-repository" className="w-full">
            <SelectValue placeholder={triggerLabel}>
              {labelRepo ? (
                <span className="flex min-w-0 items-center gap-2">
                  <Github className="h-4 w-4 shrink-0 text-muted-foreground" />
                  <span className="truncate">{labelRepo.fullName}</span>
                  {labelRepo.private && (
                    <Lock className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
                  )}
                </span>
              ) : (
                triggerLabel
              )}
            </SelectValue>
          </SelectTrigger>
          <SelectContent>
            <SelectItem value={NONE}>No repository</SelectItem>
            {repos?.map((repo) => (
              <SelectItem key={repo.id} value={repo.id}>
                <Github className="h-4 w-4 shrink-0 text-muted-foreground" />
                <span className="truncate">{repo.fullName}</span>
                {repo.private && (
                  <Lock className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
                )}
              </SelectItem>
            ))}
            {inlineRepo && (
              <SelectItem value={INLINE}>
                <Github className="h-4 w-4 shrink-0 text-muted-foreground" />
                <span className="truncate">{inlineRepo.fullName}</span>
                {inlineRepo.private && (
                  <Lock className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
                )}
              </SelectItem>
            )}
            <SelectSeparator />
            <SelectItem value={CONNECT}>
              <Plus className="h-4 w-4 shrink-0 text-muted-foreground" />
              {hasRepos
                ? `Connect another repository…`
                : `Connect a GitHub repository…`}
            </SelectItem>
          </SelectContent>
        </Select>
        {pickerOpen && (
          <GithubRepoPicker
            teamId={teamId}
            onSelect={(repo) => {
              setPickerOpen(false)
              onConnectNew(repo)
            }}
          />
        )}
      </div>

      {repoDefault && (
        <div className="space-y-2">
          <Label htmlFor="board-branch">Branch</Label>
          {inlineRepo || !selectedRepo ? (
            <Input
              id="board-branch"
              value={branch ?? ``}
              placeholder={repoDefault}
              disabled={disabled}
              className="font-mono"
              onChange={(e) => onBranchChange(e.target.value.trim() || null)}
            />
          ) : (
            <BranchCombobox
              repositoryId={selectedRepo.id}
              value={branch ?? repoDefault}
              repoDefault={repoDefault}
              disabled={disabled}
              ariaLabel="Branch"
              className="h-9 w-full justify-between font-mono text-sm font-normal"
              onPick={onBranchChange}
            />
          )}
        </div>
      )}

      <p className="text-xs text-muted-foreground">{BOARD_REPO_NOTE}</p>
      {loadError && (
        <p className="text-xs text-destructive">
          Couldn&rsquo;t load the team&rsquo;s repositories: {loadError}
        </p>
      )}
      {error && <p className="text-xs text-destructive">{error}</p>}
    </div>
  )
}
