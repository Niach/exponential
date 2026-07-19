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
// consent_code; "Allow" persists the team/board grant and completes
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

interface ScopeTeam {
  id: string
  name: string
  slug: string
  boards: Array<{ id: string; name: string; prefix: string; icon: string | null }>
}

function ConsentPage() {
  const { consent_code: consentCode, client_id: clientId } = Route.useSearch()

  const [clientName, setClientName] = useState<string | null>(null)
  const [tree, setTree] = useState<Array<ScopeTeam> | null>(null)
  const [loadError, setLoadError] = useState(``)
  const [error, setError] = useState(``)
  const [pending, setPending] = useState<`allow` | `deny` | null>(null)

  const [allTeams, setAllTeams] = useState(true)
  const [selectedTeams, setSelectedTeams] = useState<Set<string>>(
    () => new Set()
  )
  const [selectedBoards, setSelectedBoards] = useState<Set<string>>(
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
        setTree(scopes.teams)
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
    allTeams || selectedTeams.size > 0 || selectedBoards.size > 0

  // Boards inside a fully-selected team are covered by the team
  // grant — don't send them individually.
  const effectiveBoardIds = useMemo(() => {
    if (!tree) return []
    const covered = new Set(
      tree
        .filter((w) => selectedTeams.has(w.id))
        .flatMap((w) => w.boards.map((p) => p.id))
    )
    return [...selectedBoards].filter((id) => !covered.has(id))
  }, [tree, selectedTeams, selectedBoards])

  const respond = async (accept: boolean) => {
    if (!clientId || !consentCode) return
    setPending(accept ? `allow` : `deny`)
    setError(``)
    try {
      const { redirectURI } = await trpc.mcpGrants.grantAndConsent.mutate({
        clientId,
        consentCode,
        accept,
        allTeams,
        teamIds: allTeams ? [] : [...selectedTeams],
        boardIds: allTeams ? [] : effectiveBoardIds,
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
              <Label htmlFor="all-teams">Everything</Label>
              <p className="text-xs text-muted-foreground">
                All teams and boards, including ones created later.
              </p>
            </div>
            <Switch
              id="all-teams"
              checked={allTeams}
              onCheckedChange={(checked) => setAllTeams(checked === true)}
            />
          </div>

          {!allTeams && (
            <div className="max-h-72 space-y-3 overflow-y-auto rounded-md border p-3">
              {tree.length === 0 && (
                <p className="text-sm text-muted-foreground">
                  You aren&apos;t a member of any team yet.
                </p>
              )}
              {tree.map((team) => {
                const wholeTeam = selectedTeams.has(team.id)
                return (
                  <div key={team.id} className="space-y-1.5">
                    <div className="flex items-center gap-2">
                      <Checkbox
                        id={`ws-${team.id}`}
                        checked={wholeTeam}
                        onCheckedChange={(checked) => {
                          setSelectedTeams((prev) => {
                            const next = new Set(prev)
                            if (checked === true) next.add(team.id)
                            else next.delete(team.id)
                            return next
                          })
                        }}
                      />
                      <Label
                        htmlFor={`ws-${team.id}`}
                        className="font-medium"
                      >
                        {team.name}
                      </Label>
                      <span className="text-xs text-muted-foreground">
                        whole team
                      </span>
                    </div>
                    <div className="ml-6 space-y-1">
                      {team.boards.map((board) => (
                        <div key={board.id} className="flex items-center gap-2">
                          <Checkbox
                            id={`proj-${board.id}`}
                            disabled={wholeTeam}
                            checked={
                              wholeTeam || selectedBoards.has(board.id)
                            }
                            onCheckedChange={(checked) => {
                              setSelectedBoards((prev) => {
                                const next = new Set(prev)
                                if (checked === true) next.add(board.id)
                                else next.delete(board.id)
                                return next
                              })
                            }}
                          />
                          <Label
                            htmlFor={`proj-${board.id}`}
                            className="font-normal"
                          >
                            {board.name}
                            <span className="ml-1.5 text-xs text-muted-foreground">
                              {board.prefix}
                            </span>
                          </Label>
                        </div>
                      ))}
                      {team.boards.length === 0 && (
                        <p className="text-xs text-muted-foreground">
                          No boards yet.
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
            managing issues, comments, and boards. You can change the
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
