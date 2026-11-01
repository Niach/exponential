import { createFileRoute, redirect } from "@tanstack/react-router"
import { useEffect, useRef, useState } from "react"
import { fetchSessionOnce } from "@/lib/auth/client"
import { trpc } from "@/lib/trpc-client"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import {
  AuthFormShell,
  Button,
  EMPTY_SCOPE_SELECTION,
  FEEDBACK_ERRORS,
  FEEDBACK_WORDS,
  ScopePicker,
  SkeletonRows,
  effectiveScopeSelection,
  hasScopeSelection,
  type ScopePickerTeam,
  type ScopeSelection,
} from "@exp/ui"
import { pageTitle } from "@/lib/page-title"
import { AppsConsent } from "@/components/mcp/apps-consent"

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
  /** #VAPP-182: `apps` = the client connects to `/api/mcp/apps`. */
  resource?: string
}

export const Route = createFileRoute(`/auth/consent`)({
  head: () => ({ meta: [{ title: pageTitle(`Authorize`) }] }),
  component: ConsentPage,
  ssr: false,
  validateSearch: (search: Record<string, unknown>): ConsentSearch => ({
    consent_code: (search.consent_code as string) || undefined,
    client_id: (search.client_id as string) || undefined,
    scope: (search.scope as string) || undefined,
    resource: search.resource === `apps` ? `apps` : undefined,
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
  const { consent_code: consentCode, client_id: clientId, resource } = Route.useSearch()

  const [clientName, setClientName] = useState<string | null>(null)
  const [tree, setTree] = useState<Array<ScopePickerTeam> | null>(null)
  // A failed load draws the picker's empty state (+ a toast with Retry);
  // Allow stays off until the request loads.
  const [loadFailed, setLoadFailed] = useState(false)
  const [loadAttempt, setLoadAttempt] = useState(0)
  // FEED-79: the load runs on open, so a failure is quiet (the empty picker
  // + Retry); only a failed Retry tap, the person's own action, toasts.
  const retried = useRef(false)
  const retryLoad = () => {
    retried.current = true
    setLoadFailed(false)
    setLoadAttempt((n) => n + 1)
  }
  const [pending, setPending] = useState<`allow` | `deny` | null>(null)

  const [selection, setSelection] = useState<ScopeSelection>(
    EMPTY_SCOPE_SELECTION
  )

  useEffect(() => {
    if (!clientId) return
    let cancelled = false
    Promise.all([
      trpc.mcpGrants.consentInfo.query({ clientId }),
      resource === `apps` ? Promise.resolve({ teams: [] }) : trpc.mcpGrants.scopeTree.query(),
    ])
      .then(([info, scopes]) => {
        if (cancelled) return
        setClientName(info.name)
        setTree(scopes.teams)
      })
      .catch((e: unknown) => {
        if (cancelled) return
        setLoadFailed(true)
        const tapped = retried.current
        retried.current = false
        if (tapped) failedAction(`load`, `the consent request`, e, { retry: retryLoad })
      })
    return () => {
      cancelled = true
    }
  }, [clientId, loadAttempt, resource])

  const hasSelection = hasScopeSelection(selection)

  const respond = async (accept: boolean) => {
    if (!clientId || !consentCode) return
    setPending(accept ? `allow` : `deny`)
    try {
      // Boards inside a fully-selected team are covered by the team
      // grant — don't send them individually.
      const scope = effectiveScopeSelection(tree ?? [], selection)
      const { redirectURI } = await trpc.mcpGrants.grantAndConsent.mutate(
        {
          clientId,
          consentCode,
          accept,
          ...scope,
        },
        SKIP_ERROR_TOAST
      )
      window.location.href = redirectURI
    } catch (e) {
      failedAction(accept ? `allow` : `deny`, `access`, e)
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

  if (resource === `apps`) {
    return (
      <AuthFormShell
        title={`${clientName ?? `MCP client`} wants to use your apps`}
        description={`Choose which apps ${clientName ?? `this MCP client`} can use on your behalf.`}
        footer={null}
      >
        <AppsConsent clientId={clientId} clientName={clientName} consentCode={consentCode} />
      </AuthFormShell>
    )
  }

  return (
    <AuthFormShell
      title={`Authorize ${clientName ?? `MCP client`}`}
      description={`Choose what ${clientName ?? `this MCP client`} can access on your behalf.`}
      footer={null}
    >
      {!tree && !loadFailed ? (
        <SkeletonRows rowHeight={32} className="rounded-md border" />
      ) : (
        <div className="space-y-4">
          <ScopePicker
            tree={tree ?? []}
            value={selection}
            onChange={setSelection}
            idPrefix="consent"
          />
          {loadFailed && (
            <Button type="button" variant="outline" size="sm" onClick={retryLoad}>
              {FEEDBACK_ERRORS.retry}
            </Button>
          )}

          <p className="text-xs text-muted-foreground">
            The client acts as you within the selected scope: reading and
            managing issues, comments, and boards. You can change the
            selection any time by re-authenticating.
          </p>

          <div className="flex gap-2">
            <Button
              type="button"
              variant="outline"
              className="flex-1"
              pending={pending === `deny`}
              disabled={pending !== null}
              onClick={() => respond(false)}
            >
              Deny
            </Button>
            <Button
              type="button"
              className="flex-1"
              pending={pending === `allow`}
              pendingLabel={FEEDBACK_WORDS.pending.connect}
              disabled={pending !== null || !hasSelection || !tree}
              onClick={() => respond(true)}
            >
              Allow access
            </Button>
          </div>
        </div>
      )}
    </AuthFormShell>
  )
}
