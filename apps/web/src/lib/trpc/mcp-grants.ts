import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, asc, eq, inArray, isNull } from "drizzle-orm"
import { db } from "@/db/connection"
import {
  mcpGrants,
  oauthApplications,
  boards,
  teamMembers,
  teams,
} from "@/db/schema"
import { auth } from "@/lib/auth"
import { authedProcedure, router } from "@/lib/trpc"

// Backs the /auth/consent page of the MCP OAuth flow: what the client is,
// what the user can grant, and the accept/deny action that both persists the
// team/board selection (mcp_grants) and completes the better-auth
// consent hop. Consent completion — which mints the authorization code — runs
// FIRST; the grant upsert runs only after it succeeds, so a failed consent
// never rewrites an existing grant. This stays race-free: the code value
// reaches the MCP client only through the redirectURI this mutation returns,
// and the upsert completes before the mutation returns, so nothing can exchange
// the code before the grant row exists. A token whose (user, client) pair has
// no grant row resolves to no access anyway (see lib/mcp/scope.ts).

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
          name: oauthApplications.name,
          icon: oauthApplications.icon,
          disabled: oauthApplications.disabled,
        })
        .from(oauthApplications)
        .where(eq(oauthApplications.clientId, input.clientId))
        .limit(1)
      if (!client || client.disabled) {
        throw new TRPCError({
          code: `NOT_FOUND`,
          message: `Unknown OAuth client — re-run authentication from your MCP client.`,
        })
      }
      return { name: client.name ?? `MCP client`, icon: client.icon }
    }),

  // Everything the signed-in user could grant: their teams with each
  // team's non-archived boards.
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
          isNull(boards.archivedAt),
          isNull(boards.deletedAt)
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
      z.object({
        clientId: z.string().min(1).max(255),
        consentCode: z.string().min(1).max(255),
        accept: z.boolean(),
        allTeams: z.boolean().default(false),
        teamIds: z.array(z.string().uuid()).max(500).default([]),
        boardIds: z.array(z.string().uuid()).max(2000).default([]),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const completeConsent = async () => {
        const result = await auth.api.oAuthConsent({
          body: { accept: input.accept, consent_code: input.consentCode },
          headers: ctx.request.headers,
        })
        const redirectURI = (result as { redirectURI?: string }).redirectURI
        if (!redirectURI) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `The consent request is no longer valid — re-run authentication from your MCP client.`,
          })
        }
        return { redirectURI }
      }

      if (!input.accept) return completeConsent()

      const userId = ctx.session.user.id
      if (
        !input.allTeams &&
        input.teamIds.length === 0 &&
        input.boardIds.length === 0
      ) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `Select at least one team or board, or deny access.`,
        })
      }

      const [client] = await db
        .select({ clientId: oauthApplications.clientId })
        .from(oauthApplications)
        .where(eq(oauthApplications.clientId, input.clientId))
        .limit(1)
      if (!client) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `Unknown OAuth client` })
      }

      // Clamp the selection to what the user is actually a member of — the
      // page sends ids, but the grant must never exceed membership.
      const memberTeamIds = new Set(
        (await getMemberTeams(userId)).map((w) => w.id)
      )
      const teamIds = input.allTeams
        ? []
        : input.teamIds.filter((id) => memberTeamIds.has(id))
      let boardIds: string[] = []
      if (!input.allTeams && input.boardIds.length > 0) {
        const rows = await db
          .select({ id: boards.id, teamId: boards.teamId })
          .from(boards)
          .where(
            and(
              inArray(boards.id, input.boardIds),
              isNull(boards.deletedAt)
            )
          )
        boardIds = rows
          .filter((p) => memberTeamIds.has(p.teamId))
          .map((p) => p.id)
      }
      if (!input.allTeams && teamIds.length === 0 && boardIds.length === 0) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `None of the selected teams/boards are accessible to your account.`,
        })
      }

      // Consent first: this mints the authorization code and throws when the
      // consent code is expired or already redeemed, in which case the existing
      // grant must stay untouched. The code is handed to the client only via
      // the returned redirectURI, so writing the grant before returning still
      // closes the mint→grant gap. If this upsert somehow fails after a
      // successful consent, the code is consumed but never delivered and the
      // user re-runs the flow; the old (or absent) grant keeps governing any
      // token — do not "fix" this by reordering.
      const consent = await completeConsent()

      await db
        .insert(mcpGrants)
        .values({
          userId,
          clientId: input.clientId,
          allTeams: input.allTeams,
          teamIds,
          boardIds,
        })
        .onConflictDoUpdate({
          target: [mcpGrants.userId, mcpGrants.clientId],
          set: {
            allTeams: input.allTeams,
            teamIds,
            boardIds,
            updatedAt: new Date(),
          },
        })

      return consent
    }),
})
