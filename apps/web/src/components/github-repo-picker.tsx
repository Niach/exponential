import { useCallback, useEffect, useMemo, useState } from "react"
import type { ReactNode } from "react"
import type { Board } from "@/db/schema"
import { trpc } from "@/lib/trpc-client"
import { READINESS_COPY, readinessRepoRows } from "@/lib/coding-readiness"
import type { ReadinessRepoList } from "@/hooks/use-coding-readiness"
import { isRepoFullName } from "@/lib/repo-full-name"
import {
  openGithubConnect,
  openGithubPopup,
  POPUP_BLOCKED_MESSAGE,
} from "@/lib/github-connect"
import {
  GH_CAP_NOTE,
  GH_CONNECT_GITHUB,
  GH_FOOTER_EXPLAIN,
  GH_INSTALL_ANOTHER,
  GH_INSTALL_APP,
  GH_LOOK_UP,
  GH_LOOKUP_A11Y,
  GH_LOOKUP_PLACEHOLDER,
  GH_NO_MATCH,
  GH_NONE_PUSHABLE,
  GH_PICKER_CONNECTED_CHECK,
  GH_PICKER_LOADING,
  GH_PICKER_NOT_CONFIGURED,
  GH_PICKER_NOT_INSTALLED,
  GH_PICKER_NOT_LINKED,
  GH_PICKER_RECONNECT_BANNER,
  GH_RECONNECT_GITHUB,
  GH_REFRESH,
  GH_SEARCH_PLACEHOLDER,
  GH_SUSPENDED_ACCOUNT_FALLBACK,
  ghPickerSuspendedBanner,
  githubInstallationLabel,
} from "@/lib/github-connect-copy"
import {
  Button,
  Input,
  ListEmpty,
  Pill,
  RepositoryPickerList,
  conceptIcon,
} from "@exp/ui"

// Re-exported for the hosts that already import the popup helpers from here.
export { openGithubPopup, POPUP_BLOCKED_MESSAGE } from "@/lib/github-connect"

const GithubGlyph = conceptIcon(`ui-github`)
const LoadingGlyph = conceptIcon(`ui-loading`)
const ExternalLinkGlyph = conceptIcon(`ui-external-link`)
const AddGlyph = conceptIcon(`ui-add`)
const RefreshGlyph = conceptIcon(`ui-refresh`)
const WarningGlyph = conceptIcon(`ui-warning`)

export type PickerRepo = {
  fullName: string
  private: boolean
  defaultBranch: string
  installationId: number
}

export type ReposResult = Awaited<
  ReturnType<typeof trpc.integrations.github.repos.query>
>

// SLOP-7: the repo-first connect surface shared by Settings › Repositories'
// Add-repository dialog, the board form, the onboarding board step and the
// readiness checklist's "Add another repository from GitHub…". Self-
// contained: it lists the repositories the VIEWER can push to, LIVE off
// GitHub (`integrations.github.repos`), and when a prerequisite is missing it
// says which one and offers the fix — Connect GitHub (the guided page in a
// popup) or Install the app (GitHub's install page in a popup) — then
// re-lists when focus comes back. Calls `onSelect` with the chosen repo —
// FEED-42: every host treats that as the add itself (tap adds, ×4), so there
// is no selection marker.
//
// `variant="plain"` drops the list's own card chrome for hosts that already
// provide a glass surface (the Add-repository dialog); inline hosts keep the
// default glass card.

// One-step connect for hosts whose compact "Connect a GitHub repository"
// trigger expands into this picker (EXP-390): prefetches the viewer's connect
// state so the trigger click can open the guided page DIRECTLY when GitHub
// is not linked yet, instead of expanding to a second "Connect GitHub"
// button. Callers still expand the picker on the same click — it's the return
// surface whose window-focus listener re-detects the connection. `enabled`
// gates the prefetch for hosts that mount closed (dialogs).
export function useGithubConnectShortcut(teamId: string, enabled = true) {
  const [data, setData] = useState<ReposResult | null>(null)

  useEffect(() => {
    if (!enabled) return
    let active = true
    void trpc.integrations.github.repos
      .query({ teamId })
      .then((result) => {
        if (active) setData(result)
      })
      .catch(() => {})
    return () => {
      active = false
    }
  }, [teamId, enabled])

  return useCallback(() => {
    if (!data?.configured || data.installed) return
    openGithubConnect({ teamId })
  }, [data, teamId])
}

type GithubSourceProps = {
  source?: `github`
  teamId: string
  onSelect: (repo: PickerRepo) => void
  installEmptyState?: ReactNode
  variant?: `card` | `plain`
  // Optional override for the list's max-height (the ONE scroll container).
  listClassName?: string
}

type TeamSourceProps = {
  source: `team`
  teamId: string
  board: Board
  repos: ReadinessRepoList | null
  /** The board now points at a repository — close the picker. */
  onPicked: () => void
  onReload: () => void
}

/** THE repository picker of the web app: `source="github"` (default) lists the
 *  viewer's pushable repositories live off GitHub; `source="team"` lists the
 *  team's connected ones for a board (the readiness checklist's fix), with
 *  the GitHub source one footer row away. */
export function GithubRepoPicker(props: GithubSourceProps | TeamSourceProps) {
  return props.source === `team` ? <TeamSource {...props} /> : <GithubSource {...props} />
}

function GithubSource({
  teamId,
  onSelect,
  installEmptyState,
  variant = `card`,
  listClassName,
}: GithubSourceProps) {
  const [data, setData] = useState<ReposResult | null>(null)
  const [loading, setLoading] = useState(true)
  const [popupBlocked, setPopupBlocked] = useState(false)
  // FEED-30: the "Add by name" escape hatch's own state.
  const [lookupName, setLookupName] = useState(``)
  const [lookupBusy, setLookupBusy] = useState(false)
  const [lookupError, setLookupError] = useState<string | null>(null)

  const refresh = useCallback(
    async (force = false) => {
      setLoading(true)
      try {
        setData(
          await trpc.integrations.github.repos.query({
            teamId,
            ...(force ? { refresh: true } : {}),
          })
        )
      } catch {
        // Leave `data` as-is; the configured/linked branches degrade safely.
      } finally {
        setLoading(false)
      }
    },
    [teamId]
  )

  useEffect(() => {
    void refresh()
  }, [refresh])

  // Re-detect the connection after the user returns from a GitHub hop —
  // avoids brittle popup postMessage relays.
  useEffect(() => {
    const onFocus = () => void refresh(true)
    window.addEventListener(`focus`, onFocus)
    return () => window.removeEventListener(`focus`, onFocus)
  }, [refresh])

  // The guided page (Connect GitHub, then Install the app) in a popup.
  const openConnect = () => {
    setPopupBlocked(!openGithubConnect({ teamId }))
  }

  // GitHub's account picker (installations/new) — the way to a second
  // account/org once the first one is installed.
  const openInstall = () => {
    setPopupBlocked(
      !openGithubPopup(data?.installUrl) && Boolean(data?.installUrl)
    )
  }

  const lookup = async () => {
    const fullName = lookupName.trim()
    if (!isRepoFullName(fullName) || lookupBusy) return
    setLookupBusy(true)
    setLookupError(null)
    try {
      const repo = await trpc.integrations.github.lookupRepo.query({
        teamId,
        fullName,
      })
      setLookupName(``)
      onSelect(repo)
    } catch (err) {
      setLookupError(err instanceof Error ? err.message : String(err))
    } finally {
      setLookupBusy(false)
    }
  }

  if (loading && !data) {
    return (
      <div className="flex items-center gap-2 rounded-md border px-3 py-6 text-sm text-muted-foreground">
        <LoadingGlyph className="h-4 w-4 animate-spin" />
        {GH_PICKER_LOADING}
      </div>
    )
  }

  // App not configured on this server → no repo can be connected here.
  if (!data || !data.configured) {
    if (installEmptyState) return <>{installEmptyState}</>
    return (
      <div className="flex items-start gap-2 rounded-md border border-dashed px-3 py-3 text-sm text-muted-foreground">
        <GithubGlyph className="mt-0.5 h-4 w-4 shrink-0" />
        <span>{GH_PICKER_NOT_CONFIGURED}</span>
      </div>
    )
  }

  // A prerequisite is missing: GitHub not linked (or its token expired), or
  // linked but the app installed nowhere the viewer can see. One sentence
  // naming it, one button fixing it, and the "I've done that" re-list.
  if (!data.linked || data.needsReconnect || !data.installed) {
    if (installEmptyState && !data.linked) return <>{installEmptyState}</>
    const needsLink = !data.linked || data.needsReconnect
    return (
      <div className="space-y-3" data-testid="repo-picker-prerequisite">
        <div className="flex items-start gap-2 rounded-md border border-dashed px-3 py-3 text-sm text-muted-foreground">
          <GithubGlyph className="mt-0.5 h-4 w-4 shrink-0" />
          <span>
            {needsLink
              ? data.needsReconnect
                ? GH_PICKER_RECONNECT_BANNER
                : GH_PICKER_NOT_LINKED
              : GH_PICKER_NOT_INSTALLED}
          </span>
        </div>
        {/* flex-wrap: narrow hosts (mobile-width dialogs) must wrap the
            refresh button instead of clipping it (EXP-390). */}
        <div className="flex flex-wrap items-center gap-2">
          {needsLink ? (
            <Button type="button" onClick={openConnect}>
              <GithubGlyph className="mr-2 h-4 w-4" />
              {data.needsReconnect ? GH_RECONNECT_GITHUB : GH_CONNECT_GITHUB}
            </Button>
          ) : (
            <Button type="button" onClick={openInstall}>
              <GithubGlyph className="mr-2 h-4 w-4" />
              {GH_INSTALL_APP}
            </Button>
          )}
          <Pill mode="action" onClick={() => void refresh(true)}>
            <RefreshGlyph />
            {GH_PICKER_CONNECTED_CHECK}
          </Pill>
        </div>
        {popupBlocked && (
          <p className="text-xs text-destructive">{POPUP_BLOCKED_MESSAGE}</p>
        )}
      </div>
    )
  }

  // Installed → the live, searchable list of push-able repositories.
  const suspendedAccounts = data.installations
    .filter((i) => i.suspended)
    .map((i) => i.accountLogin || GH_SUSPENDED_ACCOUNT_FALLBACK)
  const empty = data.repos.length === 0
  return (
    <div className="space-y-2">
      {/* Suspended installations list no repos at all — say why, or the empty
          state reads as "you have no repositories" (REV2-29). */}
      {suspendedAccounts.length > 0 && (
        <div className="flex flex-wrap items-center gap-2 rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          <WarningGlyph className="h-4 w-4 shrink-0" />
          <span className="min-w-0 flex-1">
            {ghPickerSuspendedBanner(suspendedAccounts)}
          </span>
        </div>
      )}

      {popupBlocked && (
        <p className="text-xs text-destructive">{POPUP_BLOCKED_MESSAGE}</p>
      )}

      {!empty && (
        // The shared picker body (`RepositoryPickerList`), inline — no
        // popover, this card IS the host. Tap adds, so nothing is ever "the
        // picked repo".
        <RepositoryPickerList
          rows={data.repos.map((repo) => ({
            id: repo.fullName,
            fullName: repo.fullName,
            private: repo.private,
          }))}
          onPick={(fullName) => {
            const repo = data.repos.find((r) => r.fullName === fullName)
            if (repo) onSelect(repo)
          }}
          searchPlaceholder={GH_SEARCH_PLACEHOLDER}
          emptyText={GH_NO_MATCH}
          className={variant === `plain` ? `rounded-none border-0 bg-transparent` : undefined}
          listClassName={listClassName ?? `max-h-[min(20rem,50dvh)]`}
        />
      )}

      {empty && suspendedAccounts.length === 0 && (
        <ListEmpty className="rounded-md border">{GH_NONE_PUSHABLE}</ListEmpty>
      )}

      {/* The list explains itself: a missing repo is (almost) always an
          installation whose repo selection doesn't include it, or a repo on
          an account that isn't installed at all — say so, link the exact
          GitHub page per account, and offer the fixes plus a by-name escape
          hatch that runs the connect gate's own checks (its error names the
          real reason). */}
      <div
        className="space-y-2 rounded-md border border-dashed px-3 py-2 text-xs text-muted-foreground"
        data-testid="repo-picker-footer"
      >
        <p>
          {GH_FOOTER_EXPLAIN}
          {data.installations.length > 0 && (
            <>
              {` `}
              {data.installations.map((inst, index) => (
                <span key={inst.installationId}>
                  <a
                    href={inst.manageUrl}
                    target="_blank"
                    rel="noreferrer"
                    className="inline-flex items-center gap-0.5 text-foreground underline-offset-2 hover:underline"
                  >
                    {githubInstallationLabel(inst)}
                    <ExternalLinkGlyph className="h-3 w-3" />
                  </a>
                  {index < data.installations.length - 1 && `, `}
                </span>
              ))}
            </>
          )}
        </p>
        {data.hasMore && <p>{GH_CAP_NOTE}</p>}
        <div className="flex flex-wrap items-center gap-2">
          <Pill mode="action" onClick={() => void refresh(true)} disabled={loading}>
            <RefreshGlyph />
            {GH_REFRESH}
          </Pill>
          {data.installUrl && (
            <Pill mode="action" onClick={openInstall}>
              <AddGlyph />
              {GH_INSTALL_ANOTHER}
            </Pill>
          )}
        </div>
        {/* A div, not a form: the board dialog hosts this inside its own
            form, and nested forms are invalid markup. */}
        <div className="flex items-center gap-2">
          <Input
            value={lookupName}
            onChange={(e) => {
              setLookupName(e.target.value)
              if (lookupError) setLookupError(null)
            }}
            onKeyDown={(e) => {
              if (e.key === `Enter`) {
                e.preventDefault()
                void lookup()
              }
            }}
            placeholder={GH_LOOKUP_PLACEHOLDER}
            aria-label={GH_LOOKUP_A11Y}
            spellCheck={false}
            autoCapitalize="none"
            className="h-8 font-mono text-xs"
          />
          <Pill
            mode="action"
            disabled={!isRepoFullName(lookupName.trim()) || lookupBusy}
            onClick={() => void lookup()}
          >
            {lookupBusy ? <LoadingGlyph className="animate-spin" /> : null}
            {GH_LOOK_UP}
          </Pill>
        </div>
        {lookupError && (
          <p className="text-destructive" data-testid="repo-lookup-error">
            {lookupError}
          </p>
        )}
      </div>
    </div>
  )
}

// EXP-1121: the "Choose repository" fix, INLINE in the checklist's current
// row (it replaces the fix buttons while open). The team's repositories with
// the board-name match on top ("matches board"), the rest tagged with the
// board already using them ("used by Website"); picking one points the board
// at it (`boards.setRepository`, members hold `mutate_resources`) and the
// row ticks off the synced board shape. The footer row expands the GitHub
// source in place, which connects a brand-new repository and points the
// board at it in one go.
function TeamSource({ teamId, board, repos, onPicked, onReload }: TeamSourceProps) {
  const [query, setQuery] = useState(``)
  const [busy, setBusy] = useState(false)
  const [adding, setAdding] = useState(false)
  const rows = useMemo(
    () => (repos ? readinessRepoRows(repos, board, query) : []),
    [repos, board, query]
  )
  const point = async (repositoryId: string) => {
    if (busy) return
    setBusy(true)
    try {
      // Errors toast through the client's link; the row stays as it was.
      await trpc.boards.setRepository.mutate({ boardId: board.id, repositoryId })
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
        <GithubSource
          teamId={teamId}
          variant="plain"
          listClassName="max-h-48"
          onSelect={(picked) => void connectNew(picked)}
        />
      </div>
    )
  }

  return (
    <div data-testid="readiness-repo-picker">
      <RepositoryPickerList
        rows={rows.map((row) => ({
          id: row.id,
          fullName: row.fullName,
          tag: row.tag,
          emphasis: row.matches,
        }))}
        onPick={(repositoryId) => void point(repositoryId)}
        searchPlaceholder={READINESS_COPY.pickerSearch}
        query={query}
        onQueryChange={setQuery}
        shouldFilter={false}
        loading={repos === null}
        emptyText={
          repos !== null && repos.length === 0 ? READINESS_COPY.pickerEmpty : undefined
        }
        listClassName="max-h-48"
        footer={
          <Button
            variant="ghost"
            size="sm"
            className="w-full justify-start rounded-none text-muted-foreground"
            disabled={busy}
            onClick={() => setAdding(true)}
          >
            <AddGlyph />
            {READINESS_COPY.pickerAddFromGithub}
          </Button>
        }
      />
    </div>
  )
}
