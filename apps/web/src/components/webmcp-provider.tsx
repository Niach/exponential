import { useEffect, useState } from "react"
import { useNavigate, useParams } from "@tanstack/react-router"
import type { Team } from "@/db/schema"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import {
  setWebMcpAppContext,
  type WebMcpNavigateTarget,
} from "@/lib/webmcp/app-context"
import { WebMcpReadTools, WebMcpWriteTools } from "@/components/webmcp-tools"

// WebMCP mount (EXP-245): exposes the current team as page-scoped MCP tools
// on `document.modelContext` so in-browser agents (Chrome's origin-trial
// integration, MCP browser extensions via @mcp-b/global's tab transport) can
// read boards/issues and act as the signed-in user. Render-null, mounted in
// the team layout like FeedbackWidgetProvider. The @mcp-b/global polyfill
// preserves a native modelContext when the browser ships one; initializing
// once per page load is all it needs.
//
// We opt OUT of the package's auto-initialization and configure the transport
// ourselves, because its defaults are `allowedOrigins: ['*']` on BOTH the tab
// and the iframe transport:
//   - tab transport: every `window.postMessage` is accepted regardless of
//     origin, so any page holding a WindowProxy on this one (window.open)
//     could drive the write tools as the signed-in user — a cross-origin
//     write channel that defeats SameSite/CSRF. Pinned to our own origin;
//     extension content scripts still work (their postMessage carries the
//     page origin) as does Chrome's native `document.modelContext` path.
//   - iframe transport: responses go back to the EMBEDDER's origin, i.e. a
//     hostile framer would get full read+write. The app is never legitimately
//     framed (every other client is native), so it is disabled outright —
//     unconditionally, rather than relying on the opt-in `frame-ancestors`
//     header (SECURITY_HEADERS_ENABLED is off by default on self-hosted).
let polyfillLoaded = false

interface WebMcpUser {
  id: string
  name?: string | null
  email?: string | null
}

export function WebMcpProvider({
  team,
  user,
}: {
  team: Team
  user: WebMcpUser
}) {
  const [ready, setReady] = useState(polyfillLoaded)
  const navigate = useNavigate()
  const { boardSlug, issueIdentifier } = useParams({ strict: false })
  const { isMember } = useTeamPermissions(team)

  useEffect(() => {
    if (typeof document === `undefined` || polyfillLoaded) return
    window.__webModelContextOptions = { autoInitialize: false }
    void import(`@mcp-b/global`)
      .then(({ initializeWebModelContext }) => {
        initializeWebModelContext({
          transport: {
            tabServer: { allowedOrigins: [window.location.origin] },
            iframeServer: false,
          },
        })
        polyfillLoaded = true
        setReady(true)
      })
      .catch((error) => {
        console.warn(`[webmcp] failed to initialize`, error)
      })
  }, [])

  // Tool handlers read this store at call time — keep it fresh on every
  // route/permission change instead of re-registering the tools.
  useEffect(() => {
    const teamSlug = team.slug
    setWebMcpAppContext({
      teamId: team.id,
      teamSlug,
      teamName: team.name,
      boardSlug: boardSlug ?? null,
      issueIdentifier: issueIdentifier ?? null,
      userId: user.id,
      userName: user.name ?? null,
      userEmail: user.email ?? null,
      isMember,
      navigate: (target: WebMcpNavigateTarget) => {
        if (target.kind === `board`) {
          void navigate({
            to: `/t/$teamSlug/boards/$boardSlug`,
            params: { teamSlug, boardSlug: target.boardSlug },
          })
        } else if (target.kind === `issue`) {
          void navigate({
            to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
            params: {
              teamSlug,
              boardSlug: target.boardSlug,
              issueIdentifier: target.issueIdentifier,
            },
          })
        } else if (target.kind === `inbox`) {
          void navigate({ to: `/t/$teamSlug/inbox`, params: { teamSlug } })
        } else {
          void navigate({ to: `/t/$teamSlug/reviews`, params: { teamSlug } })
        }
      },
    })
    return () => setWebMcpAppContext(null)
  }, [team, boardSlug, issueIdentifier, isMember, navigate, user])

  if (!ready) return null
  return (
    <>
      <WebMcpReadTools />
      {isMember && <WebMcpWriteTools />}
    </>
  )
}
