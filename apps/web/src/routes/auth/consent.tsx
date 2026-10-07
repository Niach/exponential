import { createFileRoute, redirect } from "@tanstack/react-router"
import { useEffect, useState } from "react"
import { LoaderCircle } from "lucide-react"
import { fetchSessionOnce } from "@/lib/auth/client"
import { trpc } from "@/lib/trpc-client"
import {
  AuthFormShell,
  Button,
  EMPTY_SCOPE_SELECTION,
  ScopePicker,
  effectiveScopeSelection,
  hasScopeSelection,
  type ScopePickerTeam,
  type ScopeSelection,
} from "@exp/ui"
import { pageTitle } from "@/lib/page-title"

// Scope-selection consent screen for the MCP OAuth flow. The authorize
// endpoint lands here (prompt=consent is forced server-side) with a
// consent_code; "Allow" persists the team/board grant and completes
// the better-auth consent, which returns the MCP client's callback URL.
// The team/board control is `@exp/ui` ScopePicker — the same one the
// Create-API-key dialog shows (FEED-76).

interface ConsentSearch {
  consent_code?: string
  client_id?: string
  scope?: string
}

export const Route = createFileRoute(`/auth/consent`)({
  head: () => ({ meta: [{ title: pageTitle(`Authorize`) }] }),
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

function ConsentPage() {
  const { consent_code: consentCode, client_id: clientId } = Route.useSearch()

  const [clientName, setClientName] = useState<string | null>(null)
  const [tree, setTree] = useState<Array<ScopePickerTeam> | null>(null)
  const [loadError, setLoadError] = useState(``)
  const [error, setError] = useState(``)
  const [pending, setPending] = useState<`allow` | `deny` | null>(null)

  const [selection, setSelection] = useState<ScopeSelection>(
    EMPTY_SCOPE_SELECTION
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

  const hasSelection = hasScopeSelection(selection)

  const respond = async (accept: boolean) => {
    if (!clientId || !consentCode) return
    setPending(accept ? `allow` : `deny`)
    setError(``)
    try {
      // Boards inside a fully-selected team are covered by the team
      // grant — don't send them individually.
      const scope = effectiveScopeSelection(tree ?? [], selection)
      const { redirectURI } = await trpc.mcpGrants.grantAndConsent.mutate({
        clientId,
        consentCode,
        accept,
        ...scope,
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
          <LoaderCircle className="h-5 w-5 animate-spin text-muted-foreground" />
        </div>
      ) : (
        <div className="space-y-4">
          <ScopePicker
            tree={tree}
            value={selection}
            onChange={setSelection}
            idPrefix="consent"
          />

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
