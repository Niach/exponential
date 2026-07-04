import { useCallback, useEffect, useState } from "react"
import type { ReactNode } from "react"
import { Github, Loader2, Lock, RefreshCw } from "lucide-react"
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
  repos: PickerRepo[]
  hasMore: boolean
}

// Repo-first connect surface shared by workspace settings → Repositories, the
// onboarding project step, and the create-project dialog. Self-contained: loads
// the user's installable repos, offers an inline GitHub App connect when none
// are installed, and re-detects after the user returns from the GitHub install
// tab (window focus). Calls `onSelect` with the chosen repo.
//
// v4: repo-less projects no longer exist, so there is no skip escape. Surfaces
// that don't want the inline install CTA (the create-project dialog) pass
// `installEmptyState` to render their own call to action (e.g. a link to
// workspace settings → Repositories) in the App-absent states instead.
export function GithubRepoPicker({
  onSelect,
  installEmptyState,
}: {
  onSelect: (repo: PickerRepo) => void
  installEmptyState?: ReactNode
}) {
  const [data, setData] = useState<ReposResult | null>(null)
  const [loading, setLoading] = useState(true)

  const refresh = useCallback(async () => {
    setLoading(true)
    try {
      setData(await trpc.integrations.github.repos.query())
    } catch {
      // Leave `data` as-is; the configured/installed branches degrade safely.
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    void refresh()
  }, [refresh])

  // Re-detect the connection after the user returns from the GitHub install
  // tab — avoids brittle popup postMessage relays.
  useEffect(() => {
    const onFocus = () => void refresh()
    window.addEventListener(`focus`, onFocus)
    return () => window.removeEventListener(`focus`, onFocus)
  }, [refresh])

  const openInstall = () => {
    if (data?.installUrl) {
      window.open(data.installUrl, `gh-install`, `popup,width=980,height=820`)
    }
  }

  if (loading && !data) {
    return (
      <div className="flex items-center gap-2 rounded-md border px-3 py-6 text-sm text-muted-foreground">
        <Loader2 className="h-4 w-4 animate-spin" />
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

  // Configured but not installed → inline connect (or the caller's own CTA).
  if (!data.installed) {
    if (installEmptyState) return <>{installEmptyState}</>
    return (
      <div className="space-y-3">
        <div className="flex items-start gap-2 rounded-md border border-dashed px-3 py-3 text-sm text-muted-foreground">
          <Github className="mt-0.5 h-4 w-4 shrink-0" />
          <span>
            Connect the Exponential GitHub App to pick a repository to code in.
          </span>
        </div>
        <div className="flex items-center gap-2">
          <Button type="button" onClick={openInstall}>
            <Github className="mr-2 h-4 w-4" />
            Connect GitHub
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={() => void refresh()}
          >
            <RefreshCw className="mr-2 h-4 w-4" />
            I’ve connected
          </Button>
        </div>
      </div>
    )
  }

  // Installed → searchable repo list.
  return (
    <div className="space-y-2">
      <Command className="rounded-md border">
        <CommandInput placeholder="Search repositories…" />
        <CommandList>
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
                {repo.private && (
                  <Lock className="ml-auto h-3.5 w-3.5 text-muted-foreground" />
                )}
              </CommandItem>
            ))}
          </CommandGroup>
        </CommandList>
      </Command>
      <div className="flex items-center justify-between">
        <Button
          type="button"
          variant="link"
          size="sm"
          className="px-0 text-xs text-muted-foreground"
          onClick={openInstall}
        >
          {data.hasMore
            ? `Don’t see your repo? Add repos on GitHub`
            : `Add more repos on GitHub`}
        </Button>
      </div>
    </div>
  )
}
