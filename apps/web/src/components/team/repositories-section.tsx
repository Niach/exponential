import { useCallback, useEffect, useMemo, useState } from "react"
import { promptActions, removeRepositoryPrompt } from "@/lib/prompts"
import { Link, useParams } from "@tanstack/react-router"
import {
  TriangleAlert,
  Building2,
  ExternalLink,
  Github,
  Lock,
  Plus,
  RefreshCw,
  Sparkles,
  Trash2,
  User,
  X,
} from "lucide-react"
import {
  openGithubConnect,
  openGithubPopup,
  POPUP_BLOCKED_MESSAGE,
} from "@/lib/github-connect"
import { trpc } from "@/lib/trpc-client"
import { isPlanLimitError } from "@/lib/plan-limit-error"
import { trpcErrorCode } from "@/lib/trpc-error"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import {
  GH_ACCOUNTS_HEADER,
  GH_ADD_FORBIDDEN,
  GH_ADD_REPOSITORY,
  GH_CANCEL,
  GH_CONFIGURE,
  GH_CONNECT_GITHUB,
  GH_CONNECTED,
  GH_DISCONNECT,
  GH_DISCONNECT_BODY,
  GH_DISCONNECT_CONFIRM_TITLE,
  GH_INSTALL_ANOTHER,
  GH_INSTALL_APP,
  GH_INSTALLATION_CAPTION,
  GH_MANAGE,
  GH_NO_REPOSITORIES,
  GH_NOT_CONFIGURED,
  GH_NOT_INSTALLED,
  GH_NOT_LINKED,
  GH_PICKER_TITLE,
  GH_RECONNECT,
  GH_RECONNECT_GITHUB,
  GH_RECONNECT_NEEDED,
  GH_RETRY,
  GH_SECTION_TITLE,
  GH_STATUS_FAILED,
  GH_UPGRADE,
  ghConfigureTitle,
  ghConnectedAs,
  ghSuspendedLine,
  githubInstallationLabel,
} from "@/lib/github-connect-copy"
import {
  Pill,
  Button,
  Prompt,
  ListRow,
  SETTINGS_LIST_CLASS,
  GlassSectionHeader,
  EmptyState,
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
  LiveDot,
  BoardGlyph,
  BranchPicker,
  SkeletonRows,
  Spinner,
  actionErrorTitle,
  conceptIcon,
  toast,
} from "@exp/ui"
import { useTeamBoards } from "@/hooks/use-team-data"
import {
  GithubRepoPicker,
  type PickerRepo,
} from "@/components/github-repo-picker"

const UiChevronDownIcon = conceptIcon(`ui-chevron-down`)
const RepositoriesIcon = conceptIcon(`settings-repositories`)

type RepoList = Awaited<ReturnType<typeof trpc.repositories.list.query>>
type RepoRowData = RepoList[number]
type BoardRow = {
  icon?: string | null
  color?: string | null
  repositoryId?: string | null
}
type GithubStatus = Awaited<
  ReturnType<typeof trpc.integrations.github.status.query>
>
type GithubInstallation = GithubStatus[`installations`][number]

// Member-visible since EXP-557 (per-user sharing): the connection block and
// the picker show the VIEWER's own GitHub connection (SLOP-7: their linked
// GitHub account, the installations and push-able repos it sees, LIVE),
// adding shares a repo with the team, and row management (remove, branch
// pin) is sharer-or-owner.
export function TeamRepositoriesSection({
  teamId,
  currentUserId,
  isOwner,
}: {
  teamId: string
  currentUserId: string | undefined
  isOwner: boolean
}) {
  // EXP-862: the "Used by" chips render each board's own glyph in its own
  // colour, which only the synced rows carry.
  const teamBoards = useTeamBoards(teamId)
  const boardRows = useMemo(
    () => new Map<string, BoardRow>(teamBoards.map((board) => [board.id, board])),
    [teamBoards]
  )
  const [repos, setRepos] = useState<RepoList | null>(null)
  const [connectOpen, setConnectOpen] = useState(false)
  const [removeTarget, setRemoveTarget] = useState<RepoRowData | null>(null)
  // Disconnecting GitHub (unlinking the viewer's account) confirms first.
  const [disconnectOpen, setDisconnectOpen] = useState(false)
  const [busy, setBusy] = useState(false)
  // FEED-79: a failed load/add/remove/disconnect is a toast. Set when the last
  // failure was a plan cap (PRECONDITION_FAILED from lib/billing.ts) — that
  // renders the inline upgrade nudge (an upsell, not an error) instead.
  const [limitError, setLimitError] = useState<string | null>(null)
  // A failed first load = the empty state + the toast, never a skeleton
  // spinning forever.
  const [loadFailed, setLoadFailed] = useState(false)
  // The Add-repository dialog's own state (EXP-365, FEED-42): picking a row
  // (or a successful by-name lookup) adds it at once; a failure keeps the
  // dialog open and toasts (a stale GitHub grant's toast offers Reconnect
  // GitHub); a plan cap renders the upgrade nudge inside it.
  const [connectLimitError, setConnectLimitError] = useState<string | null>(
    null
  )

  // The viewer's GitHub connection: linked or not, the login, the
  // installations their token sees — drives the connection block.
  const [githubStatus, setGithubStatus] = useState<GithubStatus | null>(null)
  // EXP-774 (IDE parity): a failed probe says so instead of rendering nothing.
  const [githubStatusFailed, setGithubStatusFailed] = useState(false)

  // Billing lives on its own settings page since EXP-146 — the plan-cap
  // upgrade nudge links there instead of scrolling within this page.
  const { teamSlug } = useParams({ strict: false })

  // EXP-1274: a refetch over an already-shown list (after a write, on window
  // focus) spins the band's glyph; the rows stay.
  const [reloading, setReloading] = useState(false)
  const refresh = useCallback(async (): Promise<void> => {
    setReloading(true)
    try {
      setRepos(await trpc.repositories.list.query({ teamId }))
      setLoadFailed(false)
    } catch {
      // FEED-79: the list loads on open, on focus and after a write, so a
      // failed load is quiet: the empty state (or the rows it last had).
      setLoadFailed(true)
    } finally {
      setReloading(false)
    }
  }, [teamId])

  useEffect(() => {
    void refresh()
  }, [refresh])

  const refreshGithubStatus = useCallback(async () => {
    try {
      setGithubStatus(
        await trpc.integrations.github.status.query({ teamId })
      )
      setGithubStatusFailed(false)
    } catch {
      // Banner is a best-effort hint; the connect dialog self-detects anyway.
      setGithubStatusFailed(true)
    }
  }, [teamId])

  useEffect(() => {
    void refreshGithubStatus()
  }, [refreshGithubStatus])

  // Re-detect links + repo flags after the user returns from a GitHub popup —
  // same window-focus convention as GithubRepoPicker.
  useEffect(() => {
    const onFocus = () => {
      void refreshGithubStatus()
      void refresh()
    }
    window.addEventListener(`focus`, onFocus)
    return () => window.removeEventListener(`focus`, onFocus)
  }, [refresh, refreshGithubStatus])

  // The two GitHub hops, both popups the focus listener above re-detects:
  // the guided page (connect, then install) and GitHub's install page for a
  // further account. openGithubPopup re-focuses an already-open popup and
  // reports a popup-blocked null so the button never silently does nothing.
  const openConnect = () => {
    if (!openGithubConnect({ teamId })) {
      failedAction(`connect`, `GitHub`, POPUP_BLOCKED_MESSAGE, { retry: openConnect })
    }
  }
  const openInstall = () => {
    const url = githubStatus?.installUrl
    if (!url) return
    if (!openGithubPopup(url)) {
      failedAction(`open`, `GitHub`, POPUP_BLOCKED_MESSAGE, { retry: openInstall })
    }
  }

  // `failure` names the toast; `retry` only for the idempotent writes.
  const run = async (
    fn: () => Promise<unknown>,
    failure: { verb: string; thing: string; retry?: boolean }
  ) => {
    setBusy(true)
    setLimitError(null)
    try {
      await fn()
      await refresh()
      await refreshGithubStatus()
    } catch (err) {
      if (isPlanLimitError(err)) {
        setLimitError(err instanceof Error ? err.message : String(err))
      } else {
        failedAction(failure.verb, failure.thing, err, {
          retry: failure.retry ? () => void run(fn, failure) : undefined,
        })
      }
    } finally {
      setBusy(false)
    }
  }

  const handleConnectOpenChange = (open: boolean) => {
    setConnectOpen(open)
    if (!open) setConnectLimitError(null)
  }

  const handleAdd = async (repo: PickerRepo) => {
    if (busy) return
    setBusy(true)
    setConnectLimitError(null)
    try {
      await trpc.repositories.add.mutate(
        {
          teamId,
          fullName: repo.fullName,
          defaultBranch: repo.defaultBranch,
          private: repo.private,
        },
        // This site toasts its own failure (or the plan-cap nudge).
        SKIP_ERROR_TOAST
      )
      // Close ONLY on success — a failure keeps the dialog in front of the
      // user instead of silently vanishing (EXP-365).
      handleConnectOpenChange(false)
      await refresh()
      await refreshGithubStatus()
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      // Grant FORBIDDEN first: a stale GitHub grant must never read as an
      // upsell (desktop `is_grant_forbidden` precedent).
      if (
        trpcErrorCode(err) === `FORBIDDEN` &&
        message.toLowerCase().includes(`reconnect github`)
      ) {
        toast.error(actionErrorTitle(`add`, `the repository`), {
          description: GH_ADD_FORBIDDEN,
          action: { label: GH_RECONNECT_GITHUB, onClick: openConnect },
        })
      } else if (isPlanLimitError(err)) {
        setConnectLimitError(message)
      } else {
        failedAction(`add`, `the repository`, err)
      }
    } finally {
      setBusy(false)
    }
  }

  const handleDisconnect = () =>
    run(() => trpc.integrations.github.disconnect.mutate(undefined, SKIP_ERROR_TOAST), {
      verb: `disconnect`,
      thing: `GitHub`,
    })

  const count = repos?.length ?? 0
  const installations = githubStatus?.installations ?? []
  const manageUrlForRepo = (repo: RepoRowData) =>
    installations.find((inst) => inst.installationId === repo.installationId)
      ?.manageUrl ?? githubStatus?.installUrl ?? null
  // A suspension flags every repo of the installation as inaccessible — the
  // row's banner must then say "unsuspend", not "re-grant access" (REV2-29).
  const suspendedForRepo = (repo: RepoRowData) =>
    installations.find((inst) => inst.installationId === repo.installationId)
      ?.suspended ?? false

  const removeCopy = removeRepositoryPrompt(
    removeTarget?.fullName ?? `this repository`
  )

  return (
    <>
      <div>
        <GlassSectionHeader
          label={GH_SECTION_TITLE}
          loading={repos !== null && reloading}
          trailing={
            <Pill mode="action" onClick={() => setConnectOpen(true)}>
              <Github />
              {GH_ADD_REPOSITORY}
            </Pill>
          }
        />
        <div className="space-y-3">
          <GithubStatusLine
            status={githubStatus}
            probeFailed={githubStatusFailed}
            busy={busy}
            onRetry={() => void refreshGithubStatus()}
            onConnect={openConnect}
            onInstall={openInstall}
            onDisconnect={() => setDisconnectOpen(true)}
          />

          {limitError && (
            <div className="flex flex-wrap items-center gap-2 rounded-md border border-primary/40 bg-primary/5 px-3 py-2 text-sm">
              <Sparkles className="h-3.5 w-3.5 shrink-0 text-primary" />
              <span className="min-w-0 flex-1">{limitError}</span>
              {teamSlug && (
                <Pill asChild mode="action">
                  <Link
                    to="/t/$teamSlug/settings/billing"
                    params={{ teamSlug }}
                    hash="plans"
                  >
                    {GH_UPGRADE}
                  </Link>
                </Pill>
              )}
            </div>
          )}

          {/* EXP-1076: the ONE settings ladder — the section band over
              gapless flat rows with a hairline between each pair. */}
          <div className={SETTINGS_LIST_CLASS}>
            {repos === null && !loadFailed ? (
              // The repo row: name line over the "Used by" line.
              <SkeletonRows rowHeight={60} />
            ) : count === 0 ? (
              // EXP-1274: the shared list empty state; the copy stays the
              // GitHub contract's (`GithubCopy` ×4).
              <EmptyState
                size="list"
                icon={RepositoriesIcon}
                title={GH_NO_REPOSITORIES}
              />
            ) : (
              repos!.map((repo) => (
                <RepoRow
                  key={repo.id}
                  repo={repo}
                  boardRows={boardRows}
                  busy={busy}
                  canManage={
                    isOwner ||
                    (currentUserId != null &&
                      repo.sharedBy?.id === currentUserId)
                  }
                  manageUrl={manageUrlForRepo(repo)}
                  installationSuspended={suspendedForRepo(repo)}
                  onRemove={() => setRemoveTarget(repo)}
                  onSetDefaultBranch={(branch) =>
                    run(
                      () =>
                        trpc.repositories.setDefaultBranch.mutate(
                          { repositoryId: repo.id, branch },
                          SKIP_ERROR_TOAST
                        ),
                      { verb: `set`, thing: `the default branch`, retry: true }
                    )
                  }
                />
              ))
            )}
          </div>
        </div>
      </div>

      <Dialog open={connectOpen} onOpenChange={handleConnectOpenChange}>
        {/* overflow-hidden overrides the base DialogContent's own scroller so
            the repo list is the ONE scroll container — the nested-scroller
            combo let cmdk's autofocus shove the header off-screen (EXP-365). */}
        <DialogContent className="overflow-hidden">
          <DialogHeader>
            <DialogTitle>{GH_PICKER_TITLE}</DialogTitle>
          </DialogHeader>
          <GithubRepoPicker
            teamId={teamId}
            onSelect={(repo) => void handleAdd(repo)}
            // EXP-687: the phone presentation is a content-fitted sheet, so
            // the list takes half the viewport and the sheet grows to fit it
            // (it used to reach for the full-screen page's leftover height).
            listClassName="max-h-[min(20rem,40dvh)] max-sm:max-h-[50dvh]"
            variant="plain"
          />
          {connectLimitError && (
            <div className="flex flex-wrap items-center gap-2 rounded-md border border-primary/40 bg-primary/5 px-3 py-2 text-sm">
              <Sparkles className="h-3.5 w-3.5 shrink-0 text-primary" />
              <span className="min-w-0 flex-1">{connectLimitError}</span>
              {teamSlug && (
                <Pill asChild mode="action">
                  <Link
                    to="/t/$teamSlug/settings/billing"
                    params={{ teamSlug }}
                    hash="plans"
                  >
                    {GH_UPGRADE}
                  </Link>
                </Pill>
              )}
            </div>
          )}
          {busy && (
            <div className="flex items-center gap-2 text-sm text-muted-foreground">
              <Spinner />
            </div>
          )}
        </DialogContent>
      </Dialog>

      <Prompt
        open={removeTarget !== null}
        onOpenChange={(open) => {
          if (!open) setRemoveTarget(null)
        }}
        busy={busy}
        title={removeCopy.title}
        actions={promptActions(removeCopy, {
          remove: {
            onSelect: () => {
              const target = removeTarget
              setRemoveTarget(null)
              if (!target) return
              void run(
                () =>
                  trpc.repositories.remove.mutate(
                    { repositoryId: target.id },
                    SKIP_ERROR_TOAST
                  ),
                { verb: `remove`, thing: `the repository` }
              )
            },
          },
        })}
      />

      {/* GH_* copy is mirrored by the natives and locked by
          github-connect-copy.test.ts: kept byte-for-byte. */}
      <Prompt
        open={disconnectOpen}
        onOpenChange={setDisconnectOpen}
        busy={busy}
        title={GH_DISCONNECT_CONFIRM_TITLE}
        body={GH_DISCONNECT_BODY}
        actions={[
          { label: GH_CANCEL, role: `cancel` },
          {
            label: GH_DISCONNECT,
            role: `destructive`,
            onSelect: () => {
              setDisconnectOpen(false)
              void handleDisconnect()
            },
          },
        ]}
      />
    </>
  )
}

const installationLabel = (inst: GithubInstallation) =>
  githubInstallationLabel(inst)

// The ONE GitHub-connection surface of the section (SLOP-7). Top to bottom:
// the viewer's own connection (not connected → Connect GitHub, which opens
// the guided page; connected as `login` with Disconnect; an expired token →
// Reconnect), then the accounts where the app is installed, ONE ROW PER
// ACCOUNT with a Configure link to that installation's GitHub settings page,
// and "Install on another account" underneath. Nothing is per team anymore:
// the connection is the person's, the repositories below are the team's.
function GithubStatusLine({
  status,
  probeFailed,
  busy,
  onRetry,
  onConnect,
  onInstall,
  onDisconnect,
}: {
  status: GithubStatus | null
  probeFailed: boolean
  busy: boolean
  onRetry: () => void
  onConnect: () => void
  onInstall: () => void
  onDisconnect: () => void
}) {
  if (!status) {
    // The probe is best-effort — say nothing definite rather than a false
    // "not connected" (EXP-774, the IDE's copy). Nothing while still loading.
    if (!probeFailed) return null
    return (
      <div className="flex flex-wrap items-center gap-2 text-sm text-muted-foreground">
        <Github className="h-3.5 w-3.5 shrink-0" />
        <span className="min-w-0 flex-1">{GH_STATUS_FAILED}</span>
        <Pill mode="action" onClick={onRetry}>
          <RefreshCw />
          {GH_RETRY}
        </Pill>
      </div>
    )
  }

  if (!status.configured) {
    return (
      <div className="flex items-center gap-2 text-sm text-muted-foreground">
        <Github className="h-3.5 w-3.5 shrink-0" />
        <span>{GH_NOT_CONFIGURED}</span>
      </div>
    )
  }

  if (!status.linked) {
    return (
      <div
        className="flex flex-wrap items-center gap-2 text-sm text-muted-foreground"
        data-testid="github-connection"
        data-state="unlinked"
      >
        <Github className="h-3.5 w-3.5 shrink-0" />
        <span className="min-w-0 flex-1">{GH_NOT_LINKED}</span>
        <Pill mode="action" primary onClick={onConnect}>
          <Github />
          {GH_CONNECT_GITHUB}
        </Pill>
      </div>
    )
  }

  if (status.needsReconnect) {
    return (
      <div
        className="flex flex-wrap items-center gap-2 text-sm text-muted-foreground"
        data-testid="github-connection"
        data-state="expired"
      >
        <TriangleAlert className="h-3.5 w-3.5 shrink-0 text-amber-500" />
        <span className="min-w-0 flex-1">{GH_RECONNECT_NEEDED}</span>
        <Pill mode="action" primary onClick={onConnect}>
          <RefreshCw />
          {GH_RECONNECT}
        </Pill>
        <Button
          variant="ghost"
          size="icon"
          className="h-5 w-5 shrink-0 p-0 text-muted-foreground hover:text-destructive"
          disabled={busy}
          onClick={onDisconnect}
          title={GH_DISCONNECT}
          aria-label={GH_DISCONNECT}
        >
          <X className="h-3 w-3" />
        </Button>
      </div>
    )
  }

  const installations = status.installations
  const suspended = installations.filter((inst) => inst.suspended)

  return (
    <div className="space-y-2" data-testid="github-connection" data-state="linked">
      <div className="flex items-center gap-2 text-sm">
        <LiveDot tone="live" className="shrink-0" />
        <span className="min-w-0 flex-1 truncate text-muted-foreground">
          {status.login ? ghConnectedAs(status.login) : GH_CONNECTED}
        </span>
        <Button
          variant="ghost"
          size="icon"
          className="h-5 w-5 shrink-0 p-0 text-muted-foreground hover:text-destructive"
          disabled={busy}
          onClick={onDisconnect}
          title={GH_DISCONNECT}
          aria-label={GH_DISCONNECT}
        >
          <X className="h-3 w-3" />
        </Button>
      </div>

      {/* GitHub suspended the app for an account (REV2-29): until it is
          unsuspended no token mints — say so instead of looking healthy. */}
      {suspended.length > 0 && (
        <div className="flex flex-wrap items-center gap-2 pl-5 text-sm text-destructive">
          <TriangleAlert className="h-3.5 w-3.5 shrink-0" />
          <span className="min-w-0 flex-1">
            {ghSuspendedLine(suspended.map(installationLabel))}
          </span>
          <Pill asChild mode="action">
            <a href={suspended[0]!.manageUrl} target="_blank" rel="noreferrer">
              {GH_MANAGE}
              <ExternalLink />
            </a>
          </Pill>
        </div>
      )}

      {installations.length === 0 ? (
        <div className="flex flex-wrap items-center gap-2 pl-5 text-sm text-muted-foreground">
          <span className="min-w-0 flex-1">{GH_NOT_INSTALLED}</span>
          {status.installUrl && (
            <Pill mode="action" primary onClick={onInstall}>
              <Plus />
              {GH_INSTALL_APP}
            </Pill>
          )}
        </div>
      ) : (
        <>
          <div className="pl-5 text-xs text-muted-foreground">{GH_ACCOUNTS_HEADER}</div>
          <ul className="space-y-1 pl-5">
            {installations.map((inst) => (
              <li
                key={inst.installationId}
                className="flex items-center gap-2 text-sm"
              >
                {inst.accountType === `Organization` ? (
                  <Building2 className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
                ) : (
                  <User className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
                )}
                <span className="min-w-0 flex-1 truncate text-foreground">
                  {installationLabel(inst)}
                </span>
                <a
                  href={inst.manageUrl}
                  target="_blank"
                  rel="noreferrer"
                  className="inline-flex shrink-0 items-center gap-1 text-xs text-muted-foreground hover:text-foreground"
                  title={ghConfigureTitle(installationLabel(inst))}
                >
                  {GH_CONFIGURE}
                  <ExternalLink className="h-3 w-3" />
                </a>
              </li>
            ))}
          </ul>
          <p className="pl-5 text-xs text-muted-foreground">{GH_INSTALLATION_CAPTION}</p>
          {status.installUrl && (
            <div className="flex flex-wrap items-center gap-2 pl-5">
              <Pill mode="action" onClick={onInstall}>
                <Plus />
                {GH_INSTALL_ANOTHER}
              </Pill>
            </div>
          )}
        </>
      )}
    </div>
  )
}

function RepoRow({
  repo,
  boardRows,
  busy,
  canManage,
  manageUrl,
  installationSuspended,
  onRemove,
  onSetDefaultBranch,
}: {
  repo: RepoRowData
  /** The team's synced board rows, keyed by id — the "Used by" chips draw
   *  each board's own icon tinted with its colour (EXP-862, ×4), and the
   *  repo list only carries id/name/slug. */
  boardRows: Map<string, BoardRow>
  busy: boolean
  // Sharer-or-owner (EXP-557): remove and the branch pin. Everyone else gets
  // a read-only row (they can still code on the shared repo).
  canManage: boolean
  manageUrl: string | null
  installationSuspended: boolean
  onRemove: () => void
  onSetDefaultBranch: (branch: string | null) => void
}) {
  const inUse = repo.boards.length > 0

  return (
    // `gap-2` is the old `space-y-2` between the row's three stacked blocks;
    // `items-stretch` undoes the base row's centring so they fill the width.
    <ListRow className="flex-col items-stretch gap-2 px-3 py-2">
      <div className="flex items-center gap-2">
        <Github className="h-4 w-4 shrink-0 text-muted-foreground" />
        <span className="min-w-0 flex-1 truncate text-sm font-medium">
          {repo.fullName}
        </span>
        {canManage ? (
          <DefaultBranchMenu
            repo={repo}
            busy={busy}
            onPick={onSetDefaultBranch}
          />
        ) : (
          <Pill className="shrink-0 font-mono font-normal">
            {repo.defaultBranch}
          </Pill>
        )}
        {repo.private && (
          <Pill className="shrink-0 gap-1">
            <Lock className="h-3 w-3" />
            Private
          </Pill>
        )}
        {!canManage ? null : inUse ? (
          <TooltipProvider>
            <Tooltip>
              <TooltipTrigger asChild>
                {/* Wrapper span: a disabled button doesn't fire the pointer
                    events the tooltip trigger relies on. */}
                <span className="shrink-0">
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    disabled
                    aria-label="Remove repository"
                  >
                    <Trash2 />
                  </Button>
                </span>
              </TooltipTrigger>
              <TooltipContent>
                In use by {repo.boards.length}{` `}
                {repo.boards.length === 1 ? `board` : `boards`}. Change their
                repository first.
              </TooltipContent>
            </Tooltip>
          </TooltipProvider>
        ) : (
          <Button
            variant="ghost"
            size="icon-sm"
            className="shrink-0 hover:text-destructive"
            disabled={busy}
            onClick={onRemove}
            title="Remove repository"
          >
            <Trash2 />
          </Button>
        )}
      </div>

      {(repo.inaccessibleAt || installationSuspended) && (
        <div className="ml-6 flex flex-wrap items-center gap-2 rounded-md border border-destructive/50 bg-destructive/10 px-2.5 py-1.5 text-xs text-destructive">
          <TriangleAlert className="h-3.5 w-3.5 shrink-0" />
          <span className="min-w-0 flex-1">
            {installationSuspended
              ? `GitHub suspended the Exponential app for this repository's account. Unsuspend it on GitHub to code on this repo again.`
              : `The GitHub App lost access to this repository. Re-grant it on GitHub.`}
          </span>
          {manageUrl && (
            <Pill asChild mode="action">
              <a href={manageUrl} target="_blank" rel="noreferrer">
                {installationSuspended ? `Unsuspend` : `Re-grant`}
                <ExternalLink />
              </a>
            </Pill>
          )}
        </div>
      )}

      <div className="flex flex-wrap items-center gap-1.5 pl-6">
        {inUse ? (
          <>
            <span className="text-xs text-muted-foreground">Used by</span>
            {repo.boards.map((board) => (
              <Pill key={board.id} className="max-w-[12rem] gap-1">
                <BoardGlyph
                  board={boardRows.get(board.id) ?? {}}
                  className="size-3"
                />
                <span className="truncate">{board.name}</span>
              </Pill>
            ))}
          </>
        ) : (
          <span className="text-xs text-muted-foreground">
            Not used by any board
          </span>
        )}
        {repo.sharedBy && (
          <span className="text-xs text-muted-foreground">
            · Shared by {repo.sharedBy.name || repo.sharedBy.email}
          </span>
        )}
      </div>
    </ListRow>
  )
}

// The row's branch badge as a picker (EXP-462): the shown value is the branch
// the product treats as the repo's default (PR base, worktree base, trunk
// sync). Picking GitHub's own default clears the pin server-side so the repo
// keeps following GitHub. The searchable control itself is shared with the
// board form (EXP-712).
function DefaultBranchMenu({
  repo,
  busy,
  onPick,
}: {
  repo: RepoRowData
  busy: boolean
  onPick: (branch: string | null) => void
}) {
  return (
    <BranchPicker
      value={repo.defaultBranch}
      defaultBranch={repo.githubDefaultBranch}
      loadBranches={() =>
        trpc.repositories.listBranches
          .query({ repositoryId: repo.id })
          .then((result) => result.branches)
      }
      onPick={onPick}
      disabled={busy}
      align="end"
      trigger={
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={busy}
          className="h-5 shrink-0 gap-1 rounded-md px-1.5 font-mono text-xs font-normal"
          aria-label={`Default branch for ${repo.fullName}`}
        >
          <span className="min-w-0 truncate">{repo.defaultBranch}</span>
          <UiChevronDownIcon className="size-3 shrink-0 text-muted-foreground" />
        </Button>
      }
    />
  )
}
