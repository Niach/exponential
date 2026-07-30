import { useCallback, useEffect, useState } from "react"
import type { ReactNode } from "react"
import { Check, Github, LoaderCircle, Lock, RefreshCw } from "lucide-react"
import { cn } from "@/lib/utils"
import { trpc } from "@/lib/trpc-client"
import { Button } from "@/components/ui/button"
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command"

export type PickerRepo = {
  fullName: string
  private: boolean
  defaultBranch: string
  installationId: number
}

type ReposResult = {
  configured: boolean
  installed: boolean
  installUrl: string | null
  connectUrl: string | null
  repos: PickerRepo[]
  hasMore: boolean
  // A linked GitHub account whose user-scoped repo grants haven't been captured
  // yet (or need refreshing), and whether GitHub has the installation suspended
  // (REV2-29 — it lists no repos until it's unsuspended). Both optional so the
  // not-configured return branches (which omit them) stay assignable.
  installations?: Array<{
    needsReauth?: boolean
    suspended?: boolean
    accountLogin?: string | null
  }>
}

// Repo-first connect surface shared by team settings → Repositories, the
// onboarding board step, and the create-board dialog. Self-contained: loads
// the team's installable repos (its linked GitHub accounts), offers an
// inline connect when none are linked, and re-detects after the user returns
// from the GitHub hop (window focus). Calls `onSelect` with the chosen repo.
//
// The connect hop prefers `connectUrl` (the OAuth claim flow — one authorize
// screen, no configure page) and falls back to `installUrl` (the install-page
// round-trip) when the instance has no OAuth client secret.
//
// v4: repo-less boards no longer exist, so there is no skip escape. Every
// surface uses the built-in inline install CTA; `installEmptyState` remains
// for callers that need a custom App-absent state.
//
// `variant="plain"` drops the Command's own card chrome for hosts that already
// provide a glass surface (the Add-repository dialog); inline hosts keep the
// default bordered card.
export function GithubRepoPicker({
  teamId,
  onSelect,
  installEmptyState,
  variant = `card`,
  selectedFullName,
  listClassName,
}: {
  teamId: string
  onSelect: (repo: PickerRepo) => void
  installEmptyState?: ReactNode
  variant?: `card` | `plain`
  // Optional selection marker for hosts that treat `onSelect` as a two-step
  // pick-then-confirm (the Add-repository dialog) — renders a check on the
  // matching row. Hosts that connect immediately just omit it.
  selectedFullName?: string | null
  // Optional override for the list's max-height (the ONE scroll container).
  listClassName?: string
}) {
  const [data, setData] = useState<ReposResult | null>(null)
  const [loading, setLoading] = useState(true)

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
        // Leave `data` as-is; the configured/installed branches degrade safely.
      } finally {
        setLoading(false)
      }
    },
    [teamId]
  )

  useEffect(() => {
    void refresh()
  }, [refresh])

  // Re-detect the connection after the user returns from the GitHub hop —
  // avoids brittle popup postMessage relays.
  useEffect(() => {
    const onFocus = () => void refresh(true)
    window.addEventListener(`focus`, onFocus)
    return () => window.removeEventListener(`focus`, onFocus)
  }, [refresh])

  const openConnect = () => {
    const url = data?.connectUrl ?? data?.installUrl
    if (url) {
      window.open(url, `gh-install`, `popup,width=980,height=820`)
    }
  }

  if (loading && !data) {
    return (
      <div className="flex items-center gap-2 rounded-md border px-3 py-6 text-sm text-muted-foreground">
        <LoaderCircle className="h-4 w-4 animate-spin" />
        Loading your GitHub repositories…
      </div>
    )
  }

  // App not configured on this server → no repo can be connected here.
  if (!data || !data.configured) {
    if (installEmptyState) return <>{installEmptyState}</>
    return (
      <div className="flex items-start gap-2 rounded-md border border-dashed px-3 py-3 text-sm text-muted-foreground">
        <Github className="mt-0.5 h-4 w-4 shrink-0" />
        <span>
          GitHub isn’t configured on this server, so repositories can’t be
          connected.
        </span>
      </div>
    )
  }

  // Configured but no GitHub account linked to this team → inline connect
  // (or the caller's own CTA).
  if (!data.installed) {
    if (installEmptyState) return <>{installEmptyState}</>
    return (
      <div className="space-y-3">
        <div className="flex items-start gap-2 rounded-md border border-dashed px-3 py-3 text-sm text-muted-foreground">
          <Github className="mt-0.5 h-4 w-4 shrink-0" />
          <span>
            Connect the Exponential GitHub App to pick a repository.
            You&rsquo;ll come right back here.
          </span>
        </div>
        <div className="flex items-center gap-2">
          <Button type="button" onClick={openConnect}>
            <Github className="mr-2 h-4 w-4" />
            Connect GitHub
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={() => void refresh(true)}
          >
            <RefreshCw className="mr-2 h-4 w-4" />
            I’ve connected — refresh
          </Button>
        </div>
      </div>
    )
  }

  // Installed → searchable repo list. We only ever show repositories a member
  // proved user-scoped access to at connect time (the grant snapshot), so a repo
  // created or shared AFTER the last connect won't appear until a reconnect
  // (the banner's re-auth) re-captures the set. `needsReauth` flags a linked
  // account whose grants haven't been captured at all.
  const suspendedAccounts = (data.installations ?? [])
    .filter((i) => i.suspended)
    .map((i) => i.accountLogin || `a connected account`)
  const reauthAccounts = (data.installations ?? [])
    .filter((i) => i.needsReauth && !i.suspended)
    .map((i) => i.accountLogin)
    .filter((login): login is string => Boolean(login))
  const needsReauth = (data.installations ?? []).some(
    (i) => i.needsReauth && !i.suspended
  )
  const empty = data.repos.length === 0
  return (
    <div className="space-y-2">
      {/* Suspended installations list no repos at all — say why, or the empty
          state reads as "you have no repositories" (REV2-29). */}
      {suspendedAccounts.length > 0 && (
        <div className="flex flex-wrap items-center gap-2 rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          <Github className="h-4 w-4 shrink-0" />
          <span className="min-w-0 flex-1">
            GitHub suspended the Exponential app for{` `}
            {suspendedAccounts.join(`, `)} — its repositories can’t be
            connected until you unsuspend it on GitHub.
          </span>
        </div>
      )}

      {needsReauth && (
        <div className="flex flex-wrap items-center gap-2 rounded-md border border-amber-500/40 bg-amber-500/5 px-3 py-2 text-sm text-muted-foreground">
          <Github className="h-4 w-4 shrink-0" />
          <span className="min-w-0 flex-1">
            {empty
              ? `Reconnect GitHub to load the repositories you can access${reauthAccounts.length > 0 ? ` from ${reauthAccounts.join(`, `)}` : ``}.`
              : `Reconnect GitHub${reauthAccounts.length > 0 ? ` (${reauthAccounts.join(`, `)})` : ``} to refresh — repos created or shared with you since your last connect won’t appear until you do.`}
          </span>
          <Button type="button" size="sm" variant="outline" onClick={openConnect}>
            <RefreshCw className="mr-2 h-4 w-4" />
            Reconnect GitHub
          </Button>
        </div>
      )}

      {!empty && (
        <Command
          className={
            variant === `plain` ? undefined : `rounded-md border bg-glass-card`
          }
        >
          <CommandInput placeholder="Search repositories…" />
          <CommandList className={cn(`max-h-[min(20rem,50dvh)]`, listClassName)}>
            <CommandEmpty>No repositories found.</CommandEmpty>
            <CommandGroup>
              {data.repos.map((repo) => (
                <CommandItem
                  key={repo.fullName}
                  value={repo.fullName}
                  onSelect={() => onSelect(repo)}
                >
                  <Github className="mr-2 h-4 w-4 shrink-0" />
                  <span className="truncate">{repo.fullName}</span>
                  <span className="ml-auto flex shrink-0 items-center gap-1.5">
                    {repo.private && (
                      <Lock className="h-3.5 w-3.5 text-muted-foreground" />
                    )}
                    {selectedFullName === repo.fullName && (
                      <Check className="h-4 w-4 text-primary" />
                    )}
                  </span>
                </CommandItem>
              ))}
            </CommandGroup>
          </CommandList>
        </Command>
      )}

      {empty && !needsReauth && suspendedAccounts.length === 0 && (
        <div className="rounded-md border px-3 py-6 text-center text-sm text-muted-foreground">
          No repositories found for your connected GitHub accounts.
        </div>
      )}
    </div>
  )
}
