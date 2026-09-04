import { useCallback, useEffect, useState } from "react"
import { Link, useParams } from "@tanstack/react-router"
import {
  TriangleAlert,
  ExternalLink,
  Github,
  LoaderCircle,
  Lock,
  Sparkles,
  Trash2,
  X,
} from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { isPlanLimitError } from "@/lib/plan-limit-error"
import { Pill } from "@/components/ui/pill"
import { Button } from "@/components/ui/button"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { GlassRow, GlassSectionHeader } from "@/components/ui/glass-rows"
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip"
import { BranchCombobox } from "@/components/branch-combobox"
import {
  GithubRepoPicker,
  openGithubPopup,
  POPUP_BLOCKED_MESSAGE,
  type PickerRepo,
} from "@/components/github-repo-picker"

type RepoList = Awaited<ReturnType<typeof trpc.repositories.list.query>>
type RepoRowData = RepoList[number]
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
  const [repos, setRepos] = useState<RepoList | null>(null)
  const [connectOpen, setConnectOpen] = useState(false)
  const [removeTarget, setRemoveTarget] = useState<RepoRowData | null>(null)
  const [disconnectTarget, setDisconnectTarget] =
    useState<GithubInstallation | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  // Set when the last failure was a plan cap (PRECONDITION_FAILED from
  // lib/billing.ts) — renders the inline upgrade nudge instead of a bare error.
  const [limitError, setLimitError] = useState<string | null>(null)
  // The Add-repository dialog's own state (EXP-365): picking a row only
  // SELECTS it; the footer button connects, and failures render inside the
  // still-open dialog instead of a card-level box behind it.
  const [pendingRepo, setPendingRepo] = useState<PickerRepo | null>(null)
  const [connectError, setConnectError] = useState<string | null>(null)
  const [connectLimitError, setConnectLimitError] = useState<string | null>(
    null
  )

  // The GitHub accounts (App installations) linked to THIS team — drives
  // the status line. Linking happens via the OAuth claim flow (connectUrl)
  // or the install-page round-trip fallback (installUrl).
  const [githubStatus, setGithubStatus] = useState<GithubStatus | null>(null)

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
    } catch {
      // Banner is a best-effort hint; the connect dialog self-detects anyway.
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
      setPendingRepo(null)
      setConnectError(null)
      setConnectLimitError(null)
    }
  }

  const handleAdd = async () => {
    if (!pendingRepo || busy) return
    setBusy(true)
    setConnectError(null)
    setConnectLimitError(null)
    try {
      await trpc.repositories.add.mutate(
        {
          teamId,
          fullName: pendingRepo.fullName,
          defaultBranch: pendingRepo.defaultBranch,
          private: pendingRepo.private,
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
      if (isPlanLimitError(err)) {
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
          label="Repositories"
          trailing={
            <Pill mode="action" onClick={() => setConnectOpen(true)}>
              <Github />
              Add repository
            </Pill>
          }
        />
        <p className="px-1 pb-2 text-xs text-foreground/50">
          Connect your GitHub repos to share them with the team — everyone can
          code on a shared repo. Point a board at one to make it the clone
          target for &ldquo;Start coding&rdquo;.
        </p>
        <div className="space-y-3">
          <GithubStatusLine
            status={githubStatus}
            busy={busy}
            canUnlink
            connectHopUrl={connectHopUrl}
            onConnect={() => openConnectHop(connectHopUrl)}
            onUnlink={handleUnlink}
            onDisconnectStale={setDisconnectTarget}
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
                <Button asChild size="sm" variant="outline">
                  <Link
                    to="/t/$teamSlug/settings/billing"
                    params={{ teamSlug }}
                    hash="plans"
                  >
                    Upgrade
                  </Link>
                </Button>
              )}
            </div>
          )}

          {count === 0 ? (
            <GlassRow className="px-3 py-2 text-sm text-muted-foreground">
              No repositories connected yet.
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
            <DialogTitle>Add repository</DialogTitle>
          </DialogHeader>
          <GithubRepoPicker
            teamId={teamId}
            onSelect={setPendingRepo}
            selectedFullName={pendingRepo?.fullName ?? null}
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
          {connectLimitError && (
            <div className="flex flex-wrap items-center gap-2 rounded-md border border-primary/40 bg-primary/5 px-3 py-2 text-sm">
              <Sparkles className="h-3.5 w-3.5 shrink-0 text-primary" />
              <span className="min-w-0 flex-1">{connectLimitError}</span>
              {teamSlug && (
                <Button asChild size="sm" variant="outline">
                  <Link
                    to="/t/$teamSlug/settings/billing"
                    params={{ teamSlug }}
                    hash="plans"
                  >
                    Upgrade
                  </Link>
                </Button>
              )}
            </div>
          )}
          <DialogFooter>
            <Button disabled={!pendingRepo || busy} onClick={handleAdd}>
              {busy ? (
                <LoaderCircle className="mr-1.5 h-3.5 w-3.5 animate-spin" />
              ) : (
                <Github className="mr-1.5 h-3.5 w-3.5" />
              )}
              Add repository
            </Button>
          </DialogFooter>
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
            <AlertDialogTitle>Disconnect GitHub account</AlertDialogTitle>
            <AlertDialogDescription>
              This removes{` `}
              {disconnectTarget
                ? installationLabel(disconnectTarget)
                : `this account`}
              {` `}from the team. Nobody&rsquo;s GitHub connection covers it, so
              no repositories are lost.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={busy}>Cancel</AlertDialogCancel>
            <AlertDialogAction
              disabled={busy}
              onClick={() => {
                const target = disconnectTarget
                setDisconnectTarget(null)
                if (!target) return
                void handleUnlink(target.installationId)
              }}
            >
              Disconnect
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  )
}

const installationLabel = (inst: GithubInstallation) =>
  inst.accountLogin ?? `installation ${inst.installationId}`

// The ONE GitHub-connection surface of the section: a status line whose single
// button flips by state (Connect GitHub / Reconnect / Manage — EXP-329). All
// button actions open the same connect hop; GitHub's side handles adding
// accounts and changing repo grants. Unlink (web-only) hides behind a
// hover/focus-revealed ✕ per login; the server CONFLICTs while repos still
// use the account and the message lands in the section's inline error box.
function GithubStatusLine({
  status,
  busy,
  canUnlink,
  connectHopUrl,
  onConnect,
  onUnlink,
  onDisconnectStale,
}: {
  status: GithubStatus | null
  busy: boolean
  canUnlink: boolean
  connectHopUrl: string | null
  onConnect: () => void
  onUnlink: (installationId: number) => void
  onDisconnectStale: (installation: GithubInstallation) => void
}) {
  if (!status) return null

  if (!status.configured) {
    return (
      <div className="flex items-center gap-2 text-sm text-muted-foreground">
        <Github className="h-3.5 w-3.5 shrink-0" />
        <span>GitHub isn&rsquo;t configured on this server.</span>
      </div>
    )
  }

  if (!status.installed) {
    return (
      <div className="flex flex-wrap items-center gap-2 text-sm text-muted-foreground">
        <Github className="h-3.5 w-3.5 shrink-0" />
        <span className="min-w-0 flex-1">No GitHub account connected</span>
        {connectHopUrl && (
          <Button size="sm" variant="outline" onClick={onConnect}>
            Connect GitHub
          </Button>
        )}
      </div>
    )
  }

  const installations = status.installations
  const suspended = installations.filter((inst) => inst.suspended)

  // GitHub suspended the App for a linked account (REV2-29). The claim link
  // survives a suspension — but until it's unsuspended no token mints, which
  // means no clone, no coding, no PRs. Say so instead of looking healthy.
  if (suspended.length > 0) {
    const manageUrl = suspended[0]!.manageUrl ?? status.installUrl
    return (
      <div className="flex flex-wrap items-center gap-2 text-sm text-destructive">
        <TriangleAlert className="h-3.5 w-3.5 shrink-0" />
        <span className="min-w-0 flex-1">
          GitHub suspended the Exponential app for{` `}
          {suspended.map(installationLabel).join(`, `)}. Unsuspend it on
          GitHub.
        </span>
        {manageUrl && (
          <Button asChild size="sm" variant="outline">
            <a href={manageUrl} target="_blank" rel="noreferrer">
              Manage
              <ExternalLink className="ml-1 h-3 w-3" />
            </a>
          </Button>
        )}
      </div>
    )
  }

  // The account line ALWAYS renders when installed (EXP-365): it carries the
  // per-account unlink ✕. STALE accounts (zero grants from anyone — EXP-557)
  // get their own line with a visible Disconnect button instead of the
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
      <div className="flex flex-wrap items-center gap-2 text-sm">
        {needingReauth.length > 0 ? (
          <TriangleAlert className="h-3.5 w-3.5 shrink-0 text-amber-500" />
        ) : (
          <span className="size-2 shrink-0 rounded-full bg-emerald-500" />
        )}
        <span className="min-w-0 flex-1 text-muted-foreground">
          GitHub:{` `}
          {installations.map((inst, index) => (
            <span key={inst.installationId}>
              <span className="group/login inline-flex items-center text-foreground">
                {installationLabel(inst)}
                {canUnlink && (
                  // Zero-width until hover/keyboard focus so the resting line
                  // reads as plain "GitHub: a, b" with no gaps.
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-4 w-0 overflow-hidden p-0 opacity-0 group-hover/login:ml-0.5 group-hover/login:w-4 group-hover/login:opacity-100 focus-visible:ml-0.5 focus-visible:w-4 focus-visible:opacity-100 text-muted-foreground hover:text-destructive"
                    disabled={busy}
                    onClick={() => onUnlink(inst.installationId)}
                    title="Disconnect this GitHub account from the team"
                  >
                    <X className="h-3 w-3" />
                  </Button>
                )}
              </span>
              {index < installations.length - 1 && `, `}
            </span>
          ))}
        </span>
        {connectHopUrl && (
          <Button size="sm" variant="ghost" onClick={onConnect}>
            Manage
          </Button>
        )}
      </div>
      {needingReauth.length > 0 && (
        <div className="flex flex-wrap items-center gap-2 text-sm text-muted-foreground">
          <span className="w-3.5 shrink-0" aria-hidden />
          <span className="min-w-0 flex-1">
            Reconnect GitHub to refresh which repositories you can access
            {` `}from {needingReauth.map(installationLabel).join(`, `)}.
          </span>
          {connectHopUrl && (
            <Button size="sm" variant="outline" onClick={onConnect}>
              Reconnect
            </Button>
          )}
        </div>
      )}
      {staleAccounts.map((inst) => (
        <div
          key={inst.installationId}
          className="flex flex-wrap items-center gap-2 text-sm text-muted-foreground"
        >
          <span className="w-3.5 shrink-0" aria-hidden />
          <span className="min-w-0 flex-1">
            No one&rsquo;s GitHub connection covers{` `}
            {installationLabel(inst)} anymore — reconnecting can&rsquo;t
            refresh it.
          </span>
          <Button
            size="sm"
            variant="outline"
            disabled={busy}
            onClick={() => onDisconnectStale(inst)}
          >
            Disconnect account
          </Button>
        </div>
      ))}
    </div>
  )
}

function RepoRow({
  repo,
  busy,
  canManage,
  manageUrl,
  installationSuspended,
  onRemove,
  onSetDefaultBranch,
}: {
  repo: RepoRowData
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
                    variant="glass"
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
            variant="glass"
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
              <Pill key={board.id} className="max-w-[12rem]">
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
