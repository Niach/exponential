import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, asc, eq, inArray } from "drizzle-orm"
import { db } from "@/db/connection"
import {
  mcpGrants,
  oauthClients,
  boards,
  teamMembers,
  teams,
} from "@/db/schema"
import { auth } from "@/lib/auth"
import { boardVisible } from "@/lib/board-visibility"
import {
  assertScopeSelectionNonEmpty,
  clampScopeSelection,
  scopeSelectionInput,
} from "@/lib/emcp/scope-selection"
import {
  appsConsentTree,
  clampAppsSelection,
  writeAppsGrant,
} from "@/lib/emcp/apps-consent"
import { authedProcedure, router, type Context } from "@/lib/trpc"

// Backs the /auth/consent page of the MCP OAuth flow: what the client is,
// what the user can grant, and the accept/deny action that both persists the
// team/board selection (mcp_grants) and completes the better-auth
// consent hop. Consent completion — which mints the authorization code — runs
// FIRST; the grant upsert runs only after it succeeds, so a failed consent
// never rewrites an existing grant. This stays race-free: the code value
// reaches the MCP client only through the redirectURI this mutation returns,
// and the upsert completes before the mutation returns, so nothing can exchange
// the code before the grant row exists. A token whose (user, client) pair has
// no grant row resolves to no access anyway (see lib/emcp/scope.ts).

async function getMemberTeams(userId: string) {
  return db
    .select({
      id: teams.id,
      name: teams.name,
      slug: teams.slug,
    })
    .from(teams)
    .innerJoin(teamMembers, eq(teamMembers.teamId, teams.id))
    .where(eq(teamMembers.userId, userId))
    .orderBy(asc(teams.name))
}

// The better-auth consent hop (oauth-provider's /oauth2/consent): mints the
// authorization code (accept) or the access_denied redirect (deny). The
// consent page hands back the SIGNED query it was redirected with; the
// provider verifies the signature and expiry, so a stale, forged or replayed
// request throws. The client the grant is written for must be the one that
// signed query names.
const consentExpired = () =>
  new TRPCError({
    code: `BAD_REQUEST`,
    message: `The consent request is no longer valid. Re-run authentication from your MCP client.`,
  })

/** Pure: the client a signed consent query names (null when it names none). */
export function consentQueryClientId(oauthQuery: string): string | null {
  return new URLSearchParams(oauthQuery.replace(/^\?/, ``)).get(`client_id`)
}

/** Before anything else: the grant is written for the client the signed
 *  query names, never for one the page claims. */
function assertConsentClient(input: { oauthQuery: string; clientId: string }) {
  if (consentQueryClientId(input.oauthQuery) !== input.clientId) throw consentExpired()
}

async function completeConsent(
  ctx: Context,
  accept: boolean,
  oauthQuery: string,
  clientId: string
): Promise<{ redirectURI: string }> {
  if (consentQueryClientId(oauthQuery) !== clientId) throw consentExpired()
  let result: unknown
  try {
    // The provider resumes /oauth2/authorize from here, which needs the
    // request itself, not only its headers.
    result = await auth.api.oauth2Consent({
      body: { accept, oauth_query: oauthQuery.replace(/^\?/, ``) },
      headers: ctx.request.headers,
      request: ctx.request,
    })
    // Handed a request, Better Auth answers with a Response.
    if (result instanceof Response) {
      if (!result.ok) throw consentExpired()
      result = await result.json()
    }
  } catch {
    throw consentExpired()
  }
  const redirectURI = (result as { url?: string }).url
  if (!redirectURI) throw consentExpired()
  return { redirectURI }
}

async function assertKnownClient(clientId: string) {
  const [client] = await db
    .select({ clientId: oauthClients.clientId })
    .from(oauthClients)
    .where(eq(oauthClients.clientId, clientId))
    .limit(1)
  if (!client) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `Unknown OAuth client` })
  }
}

export const mcpGrantsRouter = router({
  // Whether the calling user has EVER completed an MCP OAuth consent (any
  // grant row, any client). Powers the getting-started checklist's MCP entry
  // (EXP-141) — a pure existence check, no scope details.
  hasAny: authedProcedure.query(async ({ ctx }) => {
    const [row] = await db
      .select({ id: mcpGrants.id })
      .from(mcpGrants)
      .where(eq(mcpGrants.userId, ctx.session.user.id))
      .limit(1)
    return { hasAny: Boolean(row) }
  }),

  // Client display info for the consent screen. Authed-only and limited to
  // name/icon — never the secret.
  consentInfo: authedProcedure
    .input(z.object({ clientId: z.string().min(1).max(255) }))
    .query(async ({ input }) => {
      const [client] = await db
        .select({
          name: oauthClients.name,
          icon: oauthClients.icon,
          disabled: oauthClients.disabled,
        })
        .from(oauthClients)
        .where(eq(oauthClients.clientId, input.clientId))
        .limit(1)
      if (!client || client.disabled) {
        throw new TRPCError({
          code: `NOT_FOUND`,
          message: `Unknown OAuth client. Re-run authentication from your MCP client.`,
        })
      }
      return { name: client.name ?? `MCP client`, icon: client.icon }
    }),

  // Everything the signed-in user could grant: their teams with each
  // team's live (non-trashed) boards.
  scopeTree: authedProcedure.query(async ({ ctx }) => {
    const memberTeams = await getMemberTeams(ctx.session.user.id)
    if (memberTeams.length === 0) return { teams: [] }
    const boardRows = await db
      .select({
        id: boards.id,
        teamId: boards.teamId,
        name: boards.name,
        prefix: boards.prefix,
        icon: boards.icon,
        color: boards.color,
      })
      .from(boards)
      .where(
        and(
          inArray(
            boards.teamId,
            memberTeams.map((w) => w.id)
          ),
          boardVisible()
        )
      )
      .orderBy(asc(boards.sortOrder), asc(boards.name))
    return {
      teams: memberTeams.map((w) => ({
        ...w,
        boards: boardRows.filter((p) => p.teamId === w.id),
      })),
    }
  }),

  // Accept: validate the selection, complete the better-auth consent (mints the
  // code, throws on an expired/replayed consent code), and only then persist the
  // selection — the redirect carrying the code is returned after the grant row
  // is written. Deny: complete the consent negatively — no grant is written or
  // changed.
  grantAndConsent: authedProcedure
    .input(
      scopeSelectionInput.extend({
        clientId: z.string().min(1).max(255),
        // The consent page's signed query (`location.search`).
        oauthQuery: z.string().min(1).max(16_384),
        accept: z.boolean(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      assertConsentClient(input)
      const completeConsentOnce = () =>
        completeConsent(ctx, input.accept, input.oauthQuery, input.clientId)

      if (!input.accept) return completeConsentOnce()

      const userId = ctx.session.user.id
      assertScopeSelectionNonEmpty(input)

      await assertKnownClient(input.clientId)

      // Clamp the selection to what the user is actually a member of — the
      // page sends ids, but the grant must never exceed membership (the same
      // clamp a scoped API key is minted through, FEED-76).
      const memberTeamIds = new Set(
        (await getMemberTeams(userId)).map((w) => w.id)
      )
      const { allTeams, teamIds, boardIds } = await clampScopeSelection(
        db,
        memberTeamIds,
        input
      )

      // Consent first: this mints the authorization code and throws when the
      // signed consent request is expired, forged or already redeemed, in which case the existing
      // grant must stay untouched. The code is handed to the client only via
      // the returned redirectURI, so writing the grant before returning still
      // closes the mint→grant gap. If this upsert somehow fails after a
      // successful consent, the code is consumed but never delivered and the
      // user re-runs the flow; the old (or absent) grant keeps governing any
      // token — do not "fix" this by reordering.
      const consent = await completeConsentOnce()

      await db
        .insert(mcpGrants)
        .values({
          userId,
          clientId: input.clientId,
          kind: `builtin`,
          allTeams,
          teamIds,
          boardIds,
        })
        .onConflictDoUpdate({
          target: [mcpGrants.userId, mcpGrants.clientId, mcpGrants.kind],
          set: {
            allTeams,
            teamIds,
            boardIds,
            updatedAt: new Date(),
          },
        })

      return consent
    }),

  // #VAPP-182: the apps endpoint's consent (`/auth/consent?resource=apps`):
  // the member's teams with the apps they could grant, each app's tool
  // count and upstreams (the member's own connection to each).
  appsTree: authedProcedure.query(async ({ ctx }) => ({
    teams: await appsConsentTree(ctx.session.user.id),
  })),

  // #VAPP-182: Allow writes the client's `apps` grant (one team, all apps or
  // a list, Read only | Full) after the consent hop, exactly like the
  // builtin grant above; Deny writes nothing.
  grantAppsAndConsent: authedProcedure
    .input(
      z.object({
        clientId: z.string().min(1).max(255),
        // The consent page's signed query (`location.search`).
        oauthQuery: z.string().min(1).max(16_384),
        accept: z.boolean(),
        teamId: z.string().uuid().optional(),
        allApps: z.boolean().default(false),
        appIds: z.array(z.string().uuid()).max(200).default([]),
        access: z.enum([`read`, `full`]).default(`full`),
      })
    )
    .mutation(async ({ ctx, input }) => {
      assertConsentClient(input)
      if (!input.accept) return completeConsent(ctx, false, input.oauthQuery, input.clientId)
      await assertKnownClient(input.clientId)
      const selection = input.teamId
        ? await clampAppsSelection(ctx.session.user.id, {
            teamId: input.teamId,
            allApps: input.allApps,
            appIds: input.appIds,
            access: input.access,
          })
        : null
      if (!selection) {
        throw new TRPCError({ code: `BAD_REQUEST`, message: `Choose a team and at least one app.` })
      }
      // Consent first (see grantAndConsent): a failed hop never rewrites a grant.
      const consent = await completeConsent(ctx, true, input.oauthQuery, input.clientId)
      await writeAppsGrant(ctx.session.user.id, input.clientId, selection)
      return consent
    }),
})
