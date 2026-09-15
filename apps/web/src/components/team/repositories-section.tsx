import { useCallback, useEffect, useMemo, useState } from "react"
import { Link, useParams } from "@tanstack/react-router"
import {
  TriangleAlert,
  Building2,
  ExternalLink,
  Github,
  LoaderCircle,
  Lock,
  Plus,
  RefreshCw,
  Sparkles,
  Trash2,
  User,
  X,
} from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { isPlanLimitError } from "@/lib/plan-limit-error"
import { trpcErrorCode } from "@/lib/trpc-error"
import {
  GH_ADD_FORBIDDEN,
  GH_ADD_REPOSITORY,
  GH_CANCEL,
  GH_CONFIGURE,
  GH_CONNECT_ANOTHER,
  GH_CONNECT_GITHUB,
  GH_CONNECTED_HEADER,
  GH_DISCONNECT,
  GH_DISCONNECT_ACCOUNT,
  GH_DISCONNECT_CONFIRM_TITLE,
  GH_INSTALL_ON_ACCOUNT,
  GH_INSTALLATION_CAPTION,
  GH_MANAGE,
  GH_NO_REPOSITORIES,
  GH_NOT_CONFIGURED,
  GH_NOT_INSTALLED,
  GH_PICKER_TITLE,
  GH_RECONNECT,
  GH_RECONNECT_GITHUB,
  GH_REFRESH_ACCESS,
  GH_RETRY,
  GH_SECTION_INTRO,
  GH_SECTION_TITLE,
  GH_STATUS_FAILED,
  GH_UNLINK_TITLE,
  GH_UPGRADE,
  ghConfigureTitle,
  ghDisconnectLiveBody,
  ghDisconnectStaleBody,
  ghReauthLine,
  ghStaleLine,
  ghSuspendedLine,
  githubInstallationLabel,
} from "@/lib/github-connect-copy"
import {
  Pill,
  Button,
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  GlassRow,
  GlassSectionHeader,
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
  LiveDot,
} from "@exp/ui"
import { BranchCombobox } from "@/components/branch-combobox"
import { BoardGlyph } from "@/components/board-glyph"
import { useTeamBoards } from "@/hooks/use-team-data"
import {
  GithubRepoPicker,
  openGithubPopup,
  POPUP_BLOCKED_MESSAGE,
  type PickerRepo,
} from "@/components/github-repo-picker"

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

// Member-visible since EXP-557 (per-user sharing): the status line and the
// pickers show the VIEWER's own GitHub connections/repos (the server scopes
// them), connecting shares a repo with the team, and row management (remove,
// branch pin) is sharer-or-owner. Owners additionally get a Disconnect button
// on STALE accounts — linked installations no reconnect can ever refresh.
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
  // FEED-42: every unlink confirms — a live account (the row ✕) and a stale
  // one (its "Disconnect account" pill) share this dialog, differing in copy.
  const [disconnectTarget, setDisconnectTarget] = useState<{
    installation: GithubInstallation
    stale: boolean
  } | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  // Set when the last failure was a plan cap (PRECONDITION_FAILED from
  // lib/billing.ts) — renders the inline upgrade nudge instead of a bare error.
  const [limitError, setLimitError] = useState<string | null>(null)
  // The Add-repository dialog's own state (EXP-365, FEED-42): picking a row
  // (or a successful by-name lookup) adds it at once; failures render inside
  // the still-open dialog instead of a card-level box behind it. A FORBIDDEN
  // grant failure gets its own arm with a Reconnect GitHub action.
  const [connectError, setConnectError] = useState<string | null>(null)
  const [connectForbidden, setConnectForbidden] = useState(false)
  const [connectLimitError, setConnectLimitError] = useState<string | null>(
    null
  )

  // The GitHub accounts (App installations) linked to THIS team — drives
  // the status line. Linking happens via the OAuth claim flow (connectUrl)
  // or the install-page round-trip fallback (installUrl).
  const [githubStatus, setGithubStatus] = useState<GithubStatus | null>(null)
  // EXP-774 (IDE parity): a failed probe says so instead of rendering nothing.
  const [githubStatusFailed, setGithubStatusFailed] = useState(false)

  // Billing lives on its own settings page since EXP-146 — the plan-cap
  // upgrade nudge links there instead of scrolling within this page.
  const { teamSlug } = useParams({ strict: false })

  const refresh = useCallback(async () => {
    try {
      setRepos(await trpc.repositories.list.query({ teamId }))
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
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

  // Connect/install URLs carry a signed single-use state token (it drives
  // the team claim and the self-closing landing page) — never append query
  // params. The focus listener above re-detects when the popup hands focus
  // back. openGithubPopup re-focuses an already-open popup and reports a
  // popup-blocked null so the button never silently does nothing (EXP-557).
  const openConnectHop = (url: string | null | undefined) => {
    if (!url) return
    setError(openGithubPopup(url) ? null : POPUP_BLOCKED_MESSAGE)
  }

  const run = async (fn: () => Promise<unknown>) => {
    setBusy(true)
    setError(null)
    setLimitError(null)
    try {
      await fn()
      await refresh()
      await refreshGithubStatus()
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      if (isPlanLimitError(err)) {
        setLimitError(message)
      } else {
        setError(message)
      }
    } finally {
      setBusy(false)
    }
  }

  const handleConnectOpenChange = (open: boolean) => {
    setConnectOpen(open)
    if (!open) {
      setConnectError(null)
      setConnectForbidden(false)
      setConnectLimitError(null)
    }
  }

  const handleAdd = async (repo: PickerRepo) => {
    if (busy) return
    setBusy(true)
    setConnectError(null)
    setConnectForbidden(false)
    setConnectLimitError(null)
    try {
      await trpc.repositories.add.mutate(
        {
          teamId,
          fullName: repo.fullName,
          defaultBranch: repo.defaultBranch,
          private: repo.private,
        },
        // Failures render inline in the dialog; the global mutation-error
        // toast would be redundant noise.
        { context: { skipErrorToast: true } }
      )
      // Close ONLY on success — a failure keeps the dialog (and its error)
      // in front of the user instead of silently vanishing (EXP-365).
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
        setConnectForbidden(true)
      } else if (isPlanLimitError(err)) {
        setConnectLimitError(message)
      } else {
        setConnectError(message)
      }
    } finally {
      setBusy(false)
    }
  }

  const handleUnlink = (installationId: number) =>
    run(() =>
      trpc.integrations.github.unlink.mutate(
        { teamId, installationId },
        { context: { skipErrorToast: true } }
      )
    )

  const count = repos?.length ?? 0
  const connectHopUrl = githubStatus
    ? (githubStatus.connectUrl ?? githubStatus.installUrl)
    : null
  const installations = githubStatus?.installations ?? []
  const manageUrlForRepo = (repo: RepoRowData) =>
    installations.find((inst) => inst.installationId === repo.installationId)
      ?.manageUrl ?? githubStatus?.installUrl ?? null
  // A suspension flags every repo of the installation as inaccessible — the
  // row's banner must then say "unsuspend", not "re-grant access" (REV2-29).
  const suspendedForRepo = (repo: RepoRowData) =>
    installations.find((inst) => inst.installationId === repo.installationId)
      ?.suspended ?? false

  return (
    <>
      <div>
        <GlassSectionHeader
          label={GH_SECTION_TITLE}
          trailing={
            <Pill mode="action" onClick={() => setConnectOpen(true)}>
              <Github />
              {GH_ADD_REPOSITORY}
            </Pill>
          }
        />
        <p className="px-1 pb-2 text-xs text-foreground/50">
          {GH_SECTION_INTRO}
        </p>
        <div className="space-y-3">
          <GithubStatusLine
            status={githubStatus}
            probeFailed={githubStatusFailed}
            busy={busy}
            canUnlink
            connectHopUrl={connectHopUrl}
            onRetry={() => void refreshGithubStatus()}
            onConnect={() => openConnectHop(connectHopUrl)}
            onInstall={() => openConnectHop(githubStatus?.installUrl)}
            onUnlink={(installation) =>
              setDisconnectTarget({ installation, stale: false })
            }
            onDisconnectStale={(installation) =>
              setDisconnectTarget({ installation, stale: true })
            }
          />

          {error && (
            <div className="rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-sm text-destructive">
              {error}
            </div>
          )}

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

          {count === 0 ? (
            <GlassRow className="px-3 py-2 text-sm text-muted-foreground">
              {GH_NO_REPOSITORIES}
            </GlassRow>
          ) : (
            // EXP-721: every team-settings entity list is one SELF-BORDERED
            // glass row per entity (members, labels, boards, repositories),
            // never a grouped card with hairlines.
            <div className="space-y-2">
              {repos!.map((repo) => (
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
                    run(() =>
                      trpc.repositories.setDefaultBranch.mutate(
                        { repositoryId: repo.id, branch },
                        { context: { skipErrorToast: true } }
                      )
                    )
                  }
                />
              ))}
            </div>
          )}
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
          {connectError && (
            <div className="rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-sm text-destructive">
              {connectError}
            </div>
          )}
          {connectForbidden && (
            <div className="flex flex-wrap items-center gap-2 rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-sm text-destructive">
              <span className="min-w-0 flex-1">{GH_ADD_FORBIDDEN}</span>
              {connectHopUrl && (
                <Pill mode="action" onClick={() => openConnectHop(connectHopUrl)}>
                  <RefreshCw />
                  {GH_RECONNECT_GITHUB}
                </Pill>
              )}
            </div>
          )}
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
              <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
            </div>
          )}
        </DialogContent>
      </Dialog>

      <AlertDialog
        open={removeTarget !== null}
        onOpenChange={(open) => {
          if (!open) setRemoveTarget(null)
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Remove repository</AlertDialogTitle>
            <AlertDialogDescription>
              This disconnects {removeTarget?.fullName} from the team.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={busy}>Cancel</AlertDialogCancel>
            <AlertDialogAction
              disabled={busy}
              onClick={() => {
                const target = removeTarget
                setRemoveTarget(null)
                if (!target) return
                void run(() =>
                  trpc.repositories.remove.mutate(
                    { repositoryId: target.id },
                    { context: { skipErrorToast: true } }
                  )
                )
              }}
            >
              Remove
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <AlertDialog
        open={disconnectTarget !== null}
        onOpenChange={(open) => {
          if (!open) setDisconnectTarget(null)
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{GH_DISCONNECT_CONFIRM_TITLE}</AlertDialogTitle>
            <AlertDialogDescription>
              {disconnectTarget &&
                (disconnectTarget.stale
                  ? ghDisconnectStaleBody(
                      installationLabel(disconnectTarget.installation)
                    )
                  : ghDisconnectLiveBody(
                      installationLabel(disconnectTarget.installation)
                    ))}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={busy}>{GH_CANCEL}</AlertDialogCancel>
            <AlertDialogAction
              disabled={busy}
              onClick={() => {
                const target = disconnectTarget
                setDisconnectTarget(null)
                if (!target) return
                void handleUnlink(target.installation.installationId)
              }}
            >
              {GH_DISCONNECT}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  )
}

const installationLabel = (inst: GithubInstallation) =>
  githubInstallationLabel(inst)

// The ONE GitHub-connection surface of the section, pinned by FEED-42 spec A
// (×4, copy in lib/github-connect-copy.ts). FEED-31: an installation is per
// GitHub account/organization, so the installed state lists ONE ROW PER
// ACCOUNT (icon, login, a Configure link to that installation's GitHub
// settings page, an always-visible unlink ✕ that confirms) and offers TWO
// clearly separate actions underneath: "Connect another account" opens
// GitHub's account picker (`installUrl` = installations/new — the ONLY way to
// reach a second org; the OAuth hop auto-redirects on re-auth and just
// re-links what's already controlled), "Refresh access" re-runs the OAuth hop
// that re-captures the viewer's repo grants. The server CONFLICTs an unlink
// while repos still use the account and the message lands in the section's
// inline error box.
function GithubStatusLine({
  status,
  probeFailed,
  busy,
  canUnlink,
  connectHopUrl,
  onRetry,
  onConnect,
  onInstall,
  onUnlink,
  onDisconnectStale,
}: {
  status: GithubStatus | null
  probeFailed: boolean
  busy: boolean
  canUnlink: boolean
  connectHopUrl: string | null
  onRetry: () => void
  onConnect: () => void
  onInstall: () => void
  onUnlink: (installation: GithubInstallation) => void
  onDisconnectStale: (installation: GithubInstallation) => void
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

  if (!status.installed) {
    // Primary = the OAuth hop (finds installations the viewer already
    // controls; the callback sends a zero-installation user on to GitHub's
    // install page itself). The secondary goes straight to the account
    // picker — only worth a second pill when the two URLs differ.
    return (
      <div className="flex flex-wrap items-center gap-2 text-sm text-muted-foreground">
        <Github className="h-3.5 w-3.5 shrink-0" />
        <span className="min-w-0 flex-1">{GH_NOT_INSTALLED}</span>
        {connectHopUrl && (
          <Pill mode="action" onClick={onConnect}>
            <Github />
            {GH_CONNECT_GITHUB}
          </Pill>
        )}
        {status.connectUrl && status.installUrl && (
          <Pill mode="action" onClick={onInstall}>
            <Plus />
            {GH_INSTALL_ON_ACCOUNT}
          </Pill>
        )}
      </div>
    )
  }

  const installations = status.installations
  const suspended = installations.filter((inst) => inst.suspended)

  // GitHub suspended the App for a linked account (REV2-29). The claim link
  // survives a suspension — but until it's unsuspended no token mints, which
  // means no clone, no coding, no PRs. Say so instead of looking healthy (and
  // show no stale lines, FEED-42).
  if (suspended.length > 0) {
    const manageUrl = suspended[0]!.manageUrl ?? status.installUrl
    return (
      <div className="flex flex-wrap items-center gap-2 text-sm text-destructive">
        <TriangleAlert className="h-3.5 w-3.5 shrink-0" />
        <span className="min-w-0 flex-1">
          {ghSuspendedLine(suspended.map(installationLabel))}
        </span>
        {manageUrl && (
          <Pill asChild mode="action">
            <a href={manageUrl} target="_blank" rel="noreferrer">
              {GH_MANAGE}
              <ExternalLink />
            </a>
          </Pill>
        )}
      </div>
    )
  }

  // The account rows ALWAYS render when installed (EXP-365): they carry the
  // per-account unlink ✕. STALE accounts (zero grants from anyone — EXP-557)
  // get their own line with a visible Disconnect pill instead of the
  // reconnect nag: reconnecting can never refresh them, which is exactly how
  // the warning got permanent (EXP-556).
  const staleAccounts = installations.filter(
    (inst) => inst.stale && !inst.suspended
  )
  const staleIds = new Set(staleAccounts.map((inst) => inst.installationId))
  const needingReauth = installations.filter(
    (inst) => inst.needsReauth && !staleIds.has(inst.installationId)
  )

  return (
    <div className="space-y-2">
      <div className="flex items-center gap-2 text-sm">
        {needingReauth.length > 0 ? (
          <TriangleAlert className="h-3.5 w-3.5 shrink-0 text-amber-500" />
        ) : (
          <LiveDot tone="live" className="shrink-0" />
        )}
        <span className="text-muted-foreground">{GH_CONNECTED_HEADER}</span>
      </div>
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
            {inst.manageUrl && (
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
            )}
            {canUnlink && (
              <Button
                variant="ghost"
                size="icon"
                className="h-5 w-5 shrink-0 p-0 text-muted-foreground hover:text-destructive"
                disabled={busy}
                onClick={() => onUnlink(inst)}
                title={GH_UNLINK_TITLE}
                aria-label={GH_UNLINK_TITLE}
              >
                <X className="h-3 w-3" />
              </Button>
            )}
          </li>
        ))}
      </ul>
      <p className="pl-5 text-xs text-muted-foreground">
        {GH_INSTALLATION_CAPTION}
      </p>
      {(status.installUrl || status.connectUrl) && (
        <div className="flex flex-wrap items-center gap-2 pl-5">
          {status.installUrl && (
            <Pill mode="action" onClick={onInstall}>
              <Plus />
              {GH_CONNECT_ANOTHER}
            </Pill>
          )}
          {status.connectUrl && (
            <Pill mode="action" onClick={onConnect}>
              <RefreshCw />
              {GH_REFRESH_ACCESS}
            </Pill>
          )}
        </div>
      )}
      {needingReauth.length > 0 && (
        <div className="flex flex-wrap items-center gap-2 text-sm text-muted-foreground">
          <span className="w-3.5 shrink-0" aria-hidden />
          <span className="min-w-0 flex-1">
            {ghReauthLine(needingReauth.map(installationLabel))}
          </span>
          {connectHopUrl && (
            <Pill mode="action" onClick={onConnect}>
              <RefreshCw />
              {GH_RECONNECT}
            </Pill>
          )}
        </div>
      )}
      {staleAccounts.map((inst) => (
        <div
          key={inst.installationId}
          className="flex flex-wrap items-center gap-2 text-sm text-muted-foreground"
        >
          <TriangleAlert className="h-3.5 w-3.5 shrink-0 text-amber-500" />
          <span className="min-w-0 flex-1">
            {ghStaleLine(installationLabel(inst))}
          </span>
          <Pill
            mode="action"
            disabled={busy}
            onClick={() => onDisconnectStale(inst)}
          >
            <X />
            {GH_DISCONNECT_ACCOUNT}
          </Pill>
        </div>
      ))}
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
    <GlassRow className="flex-col items-stretch gap-2 px-3 py-2.5">
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
    </GlassRow>
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
    <BranchCombobox
      repositoryId={repo.id}
      value={repo.defaultBranch}
      repoDefault={repo.githubDefaultBranch}
      onPick={onPick}
      disabled={busy}
      size="sm"
      align="end"
      className="h-5 shrink-0 gap-1 rounded-md px-1.5 font-mono text-xs font-normal"
      ariaLabel={`Default branch for ${repo.fullName}`}
    />
  )
}
