import { useCallback, useEffect, useRef, useState } from "react"
import { trpc } from "@/lib/trpc-client"
import { BOARD_REPO_NOTE } from "@/lib/board-copy"
import {
  BranchPicker,
  Input,
  Label,
  GlassGroup,
  Picker,
  PickerTrigger,
  conceptIcon,
  type PickerItem,
} from "@exp/ui"
import {
  GithubRepoPicker,
  type PickerRepo,
} from "@/components/github-repo-picker"

type RepoList = Awaited<ReturnType<typeof trpc.repositories.list.query>>
export type ConnectedRepo = RepoList[number]

const CONNECT = `connect`
const INLINE = `inline`

// The board form's repository + branch block (EXP-712), shared by the
// create-board dialog and the per-board settings page. Behaves like ONE
// picker ROW (EXP-862 — the label sits inside the row): "No repository", the
// team's connected repos, and a trailing
// "Connect another repository…" action that expands the GitHub picker
// underneath (a brand-new repo is reported through `onConnectNew`; the host
// decides whether that connects immediately or waits for submit). Below it,
// only once a repo is chosen, the branch coding sessions start from — the
// repo's default unless the board pins another. Nothing here mutates: the
// host owns persistence (create saves on submit, settings mutates per change).
const GithubGlyph = conceptIcon(`ui-github`)
const PrivateGlyph = conceptIcon(`ui-private`)
const AddGlyph = conceptIcon(`ui-add`)

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
  // — the settings page connects a new repo and the LIVE board row flips
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
  const value = inlineRepo ? INLINE : repositoryId
  const loading = repos === null
  const hasRepos = (repos?.length ?? 0) > 0 || Boolean(inlineRepo)

  // The trigger's label, rendered EXPLICITLY (a value with no matching item
  // would otherwise read as nothing). Never blank: an unknown id reads "Loading repository…" while
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

  const repoItem = (repo: {
    id: string
    fullName: string
    private: boolean
  }): PickerItem => ({
    value: repo.id,
    label: repo.fullName,
    icon: GithubGlyph,
    hint: repo.private ? <PrivateGlyph className="size-3.5" /> : undefined,
  })
  const repoItems: PickerItem[] = [
    ...(repos ?? []).map(repoItem),
    ...(inlineRepo
      ? [{ ...repoItem({ ...inlineRepo, id: INLINE }) }]
      : []),
    {
      value: CONNECT,
      label: hasRepos
        ? `Connect another repository…`
        : `Connect a GitHub repository…`,
      icon: AddGlyph,
    },
  ]

  return (
    <div className="space-y-2">
      <GlassGroup>
        {/* Picker is the only picker: the Repository row and the Branch
            row below share ONE row trigger (label leading, value trailing,
            chevron-right). */}
        <Picker
          mode="single"
          mobileTitle="Repository"
          width="md"
          value={value}
          disabled={disabled || loading}
          items={repoItems}
          noneLabel="No repository"
          onNone={() => {
            setPickerOpen(false)
            onSelectRegistry(null)
          }}
          onChange={(next) => {
            if (next === CONNECT) {
              setPickerOpen(true)
              return
            }
            setPickerOpen(false)
            if (next === INLINE) return
            const repo = repos?.find((r) => r.id === next)
            if (repo) onSelectRegistry(repo)
          }}
          trigger={
            <PickerTrigger
              id="board-repository"
              variant="row"
              label="Repository"
              aria-label="Repository"
              value={triggerLabel}
            />
          }
        />

        {repoDefault &&
          (inlineRepo || !selectedRepo ? (
            <div className="flex items-center gap-3 px-4 py-3">
              <Label
                htmlFor="board-branch"
                className="shrink-0 font-normal text-foreground"
              >
                Branch
              </Label>
              <Input
                id="board-branch"
                value={branch ?? ``}
                placeholder={repoDefault}
                disabled={disabled}
                className="h-auto min-w-0 flex-1 rounded-none border-0 bg-transparent p-0 text-right font-mono text-sm text-foreground/70 shadow-none focus-visible:border-0 focus-visible:ring-0 md:text-sm"
                onChange={(e) => onBranchChange(e.target.value.trim() || null)}
              />
            </div>
          ) : (
            <BranchPicker
              triggerVariant="row"
              label="Branch"
              value={branch ?? repoDefault}
              defaultBranch={repoDefault}
              loadBranches={() =>
                trpc.repositories.listBranches
                  .query({ repositoryId: selectedRepo.id })
                  .then((result) => result.branches)
              }
              disabled={disabled}
              onPick={onBranchChange}
            />
          ))}
      </GlassGroup>

      {pickerOpen && (
        <GithubRepoPicker
          teamId={teamId}
          onSelect={(repo) => {
            setPickerOpen(false)
            onConnectNew(repo)
          }}
        />
      )}

      <p className="px-1 text-xs text-muted-foreground">{BOARD_REPO_NOTE}</p>
      {loadError && (
        <p className="px-1 text-xs text-destructive">
          Couldn&rsquo;t load the team&rsquo;s repositories: {loadError}
        </p>
      )}
      {error && <p className="px-1 text-xs text-destructive">{error}</p>}
    </div>
  )
}
