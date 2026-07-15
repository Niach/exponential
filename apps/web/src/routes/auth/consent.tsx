import { createFileRoute, redirect } from "@tanstack/react-router"
import { useEffect, useMemo, useState } from "react"
import { Loader2 } from "lucide-react"
import { fetchSessionOnce } from "@/lib/auth/client"
import { trpc } from "@/lib/trpc-client"
import { AuthFormShell } from "@/components/auth-form-shell"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"

// Scope-selection consent screen for the MCP OAuth flow. The authorize
// endpoint lands here (prompt=consent is forced server-side) with a
// consent_code; "Allow" persists the workspace/project grant and completes
// the better-auth consent, which returns the MCP client's callback URL.

interface ConsentSearch {
  consent_code?: string
  client_id?: string
  scope?: string
}

export const Route = createFileRoute(`/auth/consent`)({
  component: ConsentPage,
  ssr: false,
  validateSearch: (search: Record<string, unknown>): ConsentSearch => ({
    consent_code: (search.consent_code as string) || undefined,
    client_id: (search.client_id as string) || undefined,
    scope: (search.scope as string) || undefined,
  }),
  beforeLoad: async ({ location }) => {
    const session = await fetchSessionOnce()
    if (!session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: location.href },
      })
    }
  },
})

interface ScopeWorkspace {
  id: string
  name: string
  slug: string
  projects: Array<{ id: string; name: string; prefix: string; type: string }>
}

function ConsentPage() {
  const { consent_code: consentCode, client_id: clientId } = Route.useSearch()

  const [clientName, setClientName] = useState<string | null>(null)
  const [tree, setTree] = useState<Array<ScopeWorkspace> | null>(null)
  const [loadError, setLoadError] = useState(``)
  const [error, setError] = useState(``)
  const [pending, setPending] = useState<`allow` | `deny` | null>(null)

  const [allWorkspaces, setAllWorkspaces] = useState(true)
  const [selectedWorkspaces, setSelectedWorkspaces] = useState<Set<string>>(
    () => new Set()
  )
  const [selectedProjects, setSelectedProjects] = useState<Set<string>>(
    () => new Set()
  )

  useEffect(() => {
    if (!clientId) return
    let cancelled = false
    Promise.all([
      trpc.mcpGrants.consentInfo.query({ clientId }),
      trpc.mcpGrants.scopeTree.query(),
    ])
      .then(([info, scopes]) => {
        if (cancelled) return
        setClientName(info.name)
        setTree(scopes.workspaces)
      })
      .catch((e: unknown) => {
        if (cancelled) return
        setLoadError(
          e instanceof Error ? e.message : `Couldn't load the consent request.`
        )
      })
    return () => {
      cancelled = true
    }
  }, [clientId])

  const hasSelection =
    allWorkspaces || selectedWorkspaces.size > 0 || selectedProjects.size > 0

  // Projects inside a fully-selected workspace are covered by the workspace
  // grant — don't send them individually.
  const effectiveProjectIds = useMemo(() => {
    if (!tree) return []
    const covered = new Set(
      tree
        .filter((w) => selectedWorkspaces.has(w.id))
        .flatMap((w) => w.projects.map((p) => p.id))
    )
    return [...selectedProjects].filter((id) => !covered.has(id))
  }, [tree, selectedWorkspaces, selectedProjects])

  const respond = async (accept: boolean) => {
    if (!clientId || !consentCode) return
    setPending(accept ? `allow` : `deny`)
    setError(``)
    try {
      const { redirectURI } = await trpc.mcpGrants.grantAndConsent.mutate({
        clientId,
        consentCode,
        accept,
        allWorkspaces,
        workspaceIds: allWorkspaces ? [] : [...selectedWorkspaces],
        projectIds: allWorkspaces ? [] : effectiveProjectIds,
      })
      window.location.href = redirectURI
    } catch (e) {
      setError(
        e instanceof Error ? e.message : `Something went wrong. Try again.`
      )
      setPending(null)
    }
  }

  if (!clientId || !consentCode) {
    return (
      <AuthFormShell
        title="Invalid consent request"
        description="This page can only be reached from an app's sign-in flow."
        footer={null}
      >
        <p className="text-sm text-muted-foreground">
          Start authentication again from your MCP client (in Claude Code: run
          /mcp, select the server, and authenticate).
        </p>
      </AuthFormShell>
    )
  }

  return (
    <AuthFormShell
      title={`Authorize ${clientName ?? `MCP client`}`}
      description={`Choose what ${clientName ?? `this MCP client`} can access on your behalf.`}
      footer={null}
    >
      {loadError ? (
        <p className="text-sm text-destructive">{loadError}</p>
      ) : !tree ? (
        <div className="flex justify-center py-6">
          <Loader2 className="h-5 w-5 animate-spin text-muted-foreground" />
        </div>
      ) : (
        <div className="space-y-4">
          <div className="flex items-start justify-between gap-3 rounded-md border p-3">
            <div className="space-y-0.5">
              <Label htmlFor="all-workspaces">Everything</Label>
              <p className="text-xs text-muted-foreground">
                All teams and projects, including ones created later.
              </p>
            </div>
            <Switch
              id="all-workspaces"
              checked={allWorkspaces}
              onCheckedChange={(checked) => setAllWorkspaces(checked === true)}
            />
          </div>

          {!allWorkspaces && (
            <div className="max-h-72 space-y-3 overflow-y-auto rounded-md border p-3">
              {tree.length === 0 && (
                <p className="text-sm text-muted-foreground">
                  You aren&apos;t a member of any team yet.
                </p>
              )}
              {tree.map((workspace) => {
                const wholeWorkspace = selectedWorkspaces.has(workspace.id)
                return (
                  <div key={workspace.id} className="space-y-1.5">
                    <div className="flex items-center gap-2">
                      <Checkbox
                        id={`ws-${workspace.id}`}
                        checked={wholeWorkspace}
                        onCheckedChange={(checked) => {
                          setSelectedWorkspaces((prev) => {
                            const next = new Set(prev)
                            if (checked === true) next.add(workspace.id)
                            else next.delete(workspace.id)
                            return next
                          })
                        }}
                      />
                      <Label
                        htmlFor={`ws-${workspace.id}`}
                        className="font-medium"
                      >
                        {workspace.name}
                      </Label>
                      <span className="text-xs text-muted-foreground">
                        whole team
                      </span>
                    </div>
                    <div className="ml-6 space-y-1">
                      {workspace.projects.map((project) => (
                        <div key={project.id} className="flex items-center gap-2">
                          <Checkbox
                            id={`proj-${project.id}`}
                            disabled={wholeWorkspace}
                            checked={
                              wholeWorkspace || selectedProjects.has(project.id)
                            }
                            onCheckedChange={(checked) => {
                              setSelectedProjects((prev) => {
                                const next = new Set(prev)
                                if (checked === true) next.add(project.id)
                                else next.delete(project.id)
                                return next
                              })
                            }}
                          />
                          <Label
                            htmlFor={`proj-${project.id}`}
                            className="font-normal"
                          >
                            {project.name}
                            <span className="ml-1.5 text-xs text-muted-foreground">
                              {project.prefix}
                            </span>
                          </Label>
                        </div>
                      ))}
                      {workspace.projects.length === 0 && (
                        <p className="text-xs text-muted-foreground">
                          No projects yet.
                        </p>
                      )}
                    </div>
                  </div>
                )
              })}
            </div>
          )}

          <p className="text-xs text-muted-foreground">
            The client acts as you within the selected scope: reading and
            managing issues, comments, and projects. You can change the
            selection any time by re-authenticating.
          </p>

          {error && <p className="text-sm text-destructive">{error}</p>}

          <div className="flex gap-2">
            <Button
              type="button"
              variant="outline"
              className="flex-1"
              disabled={pending !== null}
              onClick={() => respond(false)}
            >
              {pending === `deny` ? `Denying...` : `Deny`}
            </Button>
            <Button
              type="button"
              className="flex-1"
              disabled={pending !== null || !hasSelection}
              onClick={() => respond(true)}
            >
              {pending === `allow` ? `Authorizing...` : `Allow access`}
            </Button>
          </div>
        </div>
      )}
    </AuthFormShell>
  )
}
