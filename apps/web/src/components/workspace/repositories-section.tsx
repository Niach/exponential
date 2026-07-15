import { useCallback, useEffect, useState } from "react"
import {
  AlertTriangle,
  Building2,
  ExternalLink,
  Github,
  Lock,
  Sparkles,
  Trash2,
  Unlink,
  User,
} from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { isPlanLimitError } from "@/lib/plan-limit-error"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip"
import {
  GithubRepoPicker,
  type PickerRepo,
} from "@/components/github-repo-picker"
import { scrollToPlans } from "@/components/workspace/billing-section"

type RepoList = Awaited<ReturnType<typeof trpc.repositories.list.query>>
type RepoRowData = RepoList[number]
type GithubStatus = Awaited<
  ReturnType<typeof trpc.integrations.github.status.query>
>
type GithubInstallation = GithubStatus[`installations`][number]

export function WorkspaceRepositoriesSection({
  workspaceId,
  isFeedbackWorkspace = false,
}: {
  workspaceId: string
  // The bootstrap feedback workspace's GitHub connection is protected (server
  // refuses integrations.unlink for it) — hide the unlink affordance entirely.
  isFeedbackWorkspace?: boolean
}) {
  const [repos, setRepos] = useState<RepoList | null>(null)
  const [connectOpen, setConnectOpen] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  // Set when the last failure was a plan cap (PRECONDITION_FAILED from
  // lib/billing.ts) — renders the inline upgrade nudge instead of a bare error.
  const [limitError, setLimitError] = useState<string | null>(null)

  // The GitHub accounts (App installations) linked to THIS workspace — drives
  // the chips above Connect. Linking happens via the OAuth claim flow
  // (connectUrl) or the install-page round-trip fallback (installUrl).
  const [githubStatus, setGithubStatus] = useState<GithubStatus | null>(null)

  const refresh = useCallback(async () => {
    try {
      setRepos(await trpc.repositories.list.query({ workspaceId }))
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
  }, [workspaceId])

  useEffect(() => {
    void refresh()
  }, [refresh])

  const refreshGithubStatus = useCallback(async () => {
    try {
      setGithubStatus(
        await trpc.integrations.github.status.query({ workspaceId })
      )
    } catch {
      // Banner is a best-effort hint; the connect dialog self-detects anyway.
    }
  }, [workspaceId])

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

  const openGithubPopup = (url: string | null | undefined) => {
    if (!url) return
    // Connect/install URLs carry a signed single-use state token (it drives
    // the workspace claim and the self-closing landing page) — never append
    // query params. The focus listener above re-detects when the popup hands
    // focus back.
    window.open(url, `gh-install`, `popup,width=980,height=820`)
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

  const handleConnect = (picked: PickerRepo) =>
    run(async () => {
      try {
        await trpc.repositories.add.mutate(
          {
            workspaceId,
            fullName: picked.fullName,
            defaultBranch: picked.defaultBranch,
            private: picked.private,
          },
          // Failures render inline below (plan-limit nudge or error box);
          // the global mutation-error toast would be redundant noise.
          { context: { skipErrorToast: true } }
        )
      } finally {
        // Close the picker either way so the inline nudge/error is visible.
        setConnectOpen(false)
      }
    })

  const handleUnlink = (installationId: number) =>
    run(() =>
      trpc.integrations.github.unlink.mutate(
        { workspaceId, installationId },
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

  return (
    <>
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-base">
            Repositories
            <Badge variant="secondary" className="text-xs font-normal">
              {count}
            </Badge>
          </CardTitle>
          <CardDescription>
            Connect GitHub repos so projects in this team can be coded on.
            Point a project at a repo to make it the clone target for
            &ldquo;Start coding&rdquo;.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-3">
          {githubStatus?.configured && !githubStatus.installed && (
            <div className="flex flex-wrap items-center gap-2 rounded-md border border-dashed px-3 py-2 text-sm text-muted-foreground">
              <Github className="h-3.5 w-3.5 shrink-0" />
              <span className="min-w-0 flex-1">
                No GitHub account is connected to this team yet. Connect
                one to pick repositories here.
              </span>
              {connectHopUrl && (
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => openGithubPopup(connectHopUrl)}
                >
                  Connect GitHub
                </Button>
              )}
            </div>
          )}

          {githubStatus?.configured && installations.length > 0 && (
            <div className="space-y-1.5">
              <div className="flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground">
                <Github className="h-3.5 w-3.5 shrink-0" />
                <span>Connected GitHub accounts</span>
                {connectHopUrl && (
                  <Button
                    variant="link"
                    size="sm"
                    className="h-auto px-0 text-xs text-muted-foreground"
                    onClick={() => openGithubPopup(connectHopUrl)}
                  >
                    Connect another…
                  </Button>
                )}
              </div>
              <div className="flex flex-wrap gap-1.5">
                {installations.map((inst) => (
                  <InstallationChip
                    key={inst.installationId}
                    installation={inst}
                    busy={busy}
                    canUnlink={!isFeedbackWorkspace}
                    onUnlink={() => handleUnlink(inst.installationId)}
                  />
                ))}
              </div>
              {installations.some((inst) => inst.needsReauth) &&
                connectHopUrl && (
                  <div className="flex flex-wrap items-center gap-2 rounded-md border border-amber-500/40 bg-amber-500/5 px-3 py-2 text-xs text-muted-foreground">
                    <AlertTriangle className="h-3.5 w-3.5 shrink-0 text-amber-500" />
                    <span className="min-w-0 flex-1">
                      Reconnect GitHub to refresh which repositories you can
                      access. We only list repos you can access on GitHub, so
                      any created or shared with you since your last connect
                      won&rsquo;t appear until you reconnect.
                    </span>
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={() => openGithubPopup(connectHopUrl)}
                    >
                      <Github className="mr-1.5 h-3.5 w-3.5" />
                      Reconnect
                    </Button>
                  </div>
                )}
            </div>
          )}

          <div>
            <Button size="sm" onClick={() => setConnectOpen(true)}>
              <Github className="mr-1.5 h-3.5 w-3.5" />
              Connect repository
            </Button>
          </div>

          {error && (
            <div className="rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-sm text-destructive">
              {error}
            </div>
          )}

          {limitError && (
            <div className="flex flex-wrap items-center gap-2 rounded-md border border-primary/40 bg-primary/5 px-3 py-2 text-sm">
              <Sparkles className="h-3.5 w-3.5 shrink-0 text-primary" />
              <span className="min-w-0 flex-1">{limitError}</span>
              {/* JS scroll to the plan-comparison grid in the billing section
                  above (not an <a href="#…">) so repeat clicks always scroll
                  even when the hash is already set (EXP-35). */}
              <Button size="sm" variant="outline" onClick={scrollToPlans}>
                Upgrade
              </Button>
            </div>
          )}

          {count === 0 ? (
            <div className="rounded-md border px-3 py-2 text-sm text-muted-foreground">
              No repositories connected yet.
            </div>
          ) : (
            <div className="divide-y rounded-md border">
              {repos!.map((repo) => (
                <RepoRow
                  key={repo.id}
                  repo={repo}
                  busy={busy}
                  manageUrl={manageUrlForRepo(repo)}
                  onRemove={() =>
                    run(() =>
                      trpc.repositories.remove.mutate(
                        { repositoryId: repo.id },
                        { context: { skipErrorToast: true } }
                      )
                    )
                  }
                />
              ))}
            </div>
          )}
        </CardContent>
      </Card>

      <Dialog open={connectOpen} onOpenChange={setConnectOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Connect a repository</DialogTitle>
            <DialogDescription>
              Pick a repository from this team&apos;s connected GitHub
              accounts. It becomes available to point this team&apos;s
              projects at.
            </DialogDescription>
          </DialogHeader>
          <GithubRepoPicker workspaceId={workspaceId} onSelect={handleConnect} />
        </DialogContent>
      </Dialog>
    </>
  )
}

// One linked GitHub account: login + manage link + unlink. Unlink is blocked
// server-side (CONFLICT) while connected repos still use the account — the
// error renders in the section's inline error box.
function InstallationChip({
  installation,
  busy,
  canUnlink,
  onUnlink,
}: {
  installation: GithubInstallation
  busy: boolean
  canUnlink: boolean
  onUnlink: () => void
}) {
  return (
    <div className="flex items-center gap-1.5 rounded-md border px-2 py-1 text-xs">
      {installation.accountType === `Organization` ? (
        <Building2 className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
      ) : (
        <User className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
      )}
      <span className="font-medium">
        {installation.accountLogin ?? `Installation ${installation.installationId}`}
      </span>
      {installation.needsReauth && (
        <span title="Reconnect GitHub to load this account's repositories">
          <AlertTriangle className="h-3.5 w-3.5 shrink-0 text-amber-500" />
        </span>
      )}
      <Button
        asChild
        variant="ghost"
        size="icon"
        className="h-5 w-5 text-muted-foreground"
        title="Manage repository access on GitHub"
      >
        <a href={installation.manageUrl} target="_blank" rel="noreferrer">
          <ExternalLink className="h-3 w-3" />
        </a>
      </Button>
      {canUnlink && (
        <Button
          variant="ghost"
          size="icon"
          className="h-5 w-5 text-muted-foreground hover:text-destructive"
          disabled={busy}
          onClick={onUnlink}
          title="Disconnect this GitHub account from the team"
        >
          <Unlink className="h-3 w-3" />
        </Button>
      )}
    </div>
  )
}

function RepoRow({
  repo,
  busy,
  manageUrl,
  onRemove,
}: {
  repo: RepoRowData
  busy: boolean
  manageUrl: string | null
  onRemove: () => void
}) {
  const inUse = repo.projects.length > 0

  return (
    <div className="space-y-2 px-3 py-2.5">
      <div className="flex items-center gap-2">
        <Github className="h-4 w-4 shrink-0 text-muted-foreground" />
        <span className="min-w-0 flex-1 truncate text-sm font-medium">
          {repo.fullName}
        </span>
        <Badge variant="outline" className="shrink-0 font-mono text-xs">
          {repo.defaultBranch}
        </Badge>
        {repo.private && (
          <Badge variant="secondary" className="shrink-0 gap-1 text-xs">
            <Lock className="h-3 w-3" />
            Private
          </Badge>
        )}
        {inUse ? (
          <TooltipProvider>
            <Tooltip>
              <TooltipTrigger asChild>
                {/* Wrapper span: a disabled button doesn't fire the pointer
                    events the tooltip trigger relies on. */}
                <span className="shrink-0">
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-7 w-7 text-muted-foreground"
                    disabled
                    aria-label="Remove repository"
                  >
                    <Trash2 className="h-3.5 w-3.5" />
                  </Button>
                </span>
              </TooltipTrigger>
              <TooltipContent>
                In use by {repo.projects.length}{` `}
                {repo.projects.length === 1 ? `project` : `projects`} — change
                their repository first
              </TooltipContent>
            </Tooltip>
          </TooltipProvider>
        ) : (
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7 shrink-0 text-muted-foreground hover:text-destructive"
            disabled={busy}
            onClick={onRemove}
            title="Remove repository"
          >
            <Trash2 className="h-3.5 w-3.5" />
          </Button>
        )}
      </div>

      {repo.inaccessibleAt && (
        <div className="ml-6 flex flex-wrap items-center gap-2 rounded-md border border-destructive/50 bg-destructive/10 px-2.5 py-1.5 text-xs text-destructive">
          <AlertTriangle className="h-3.5 w-3.5 shrink-0" />
          <span className="min-w-0 flex-1">
            The GitHub App lost access to this repository — re-grant it on
            GitHub.
          </span>
          {manageUrl && (
            <Button
              asChild
              size="sm"
              variant="outline"
              className="h-6 px-2 text-xs"
            >
              <a href={manageUrl} target="_blank" rel="noreferrer">
                Re-grant
                <ExternalLink className="ml-1 h-3 w-3" />
              </a>
            </Button>
          )}
        </div>
      )}

      <div className="flex flex-wrap items-center gap-1.5 pl-6">
        {inUse ? (
          <>
            <span className="text-xs text-muted-foreground">Used by</span>
            {repo.projects.map((project) => (
              <Badge key={project.id} variant="outline" className="max-w-[12rem]">
                <span className="truncate">{project.name}</span>
              </Badge>
            ))}
          </>
        ) : (
          <span className="text-xs text-muted-foreground">
            Not used by any project
          </span>
        )}
      </div>
    </div>
  )
}
