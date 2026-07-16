import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, asc, eq, inArray, isNull } from "drizzle-orm"
import { db } from "@/db/connection"
import {
  mcpGrants,
  oauthApplications,
  projects,
  workspaceMembers,
  workspaces,
} from "@/db/schema"
import { auth } from "@/lib/auth"
import { authedProcedure, router } from "@/lib/trpc"

// Backs the /auth/consent page of the MCP OAuth flow: what the client is,
// what the user can grant, and the accept/deny action that both persists the
// workspace/project selection (mcp_grants) and completes the better-auth
// consent hop. Consent completion — which mints the authorization code — runs
// FIRST; the grant upsert runs only after it succeeds, so a failed consent
// never rewrites an existing grant. This stays race-free: the code value
// reaches the MCP client only through the redirectURI this mutation returns,
// and the upsert completes before the mutation returns, so nothing can exchange
// the code before the grant row exists. A token whose (user, client) pair has
// no grant row resolves to no access anyway (see lib/mcp/scope.ts).

async function getMemberWorkspaces(userId: string) {
  return db
    .select({
      id: workspaces.id,
      name: workspaces.name,
      slug: workspaces.slug,
    })
    .from(workspaces)
    .innerJoin(workspaceMembers, eq(workspaceMembers.workspaceId, workspaces.id))
    .where(eq(workspaceMembers.userId, userId))
    .orderBy(asc(workspaces.name))
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

  // Everything the signed-in user could grant: their workspaces with each
  // workspace's non-archived projects.
  scopeTree: authedProcedure.query(async ({ ctx }) => {
    const memberWorkspaces = await getMemberWorkspaces(ctx.session.user.id)
    if (memberWorkspaces.length === 0) return { workspaces: [] }
    const projectRows = await db
      .select({
        id: projects.id,
        workspaceId: projects.workspaceId,
        name: projects.name,
        prefix: projects.prefix,
        icon: projects.icon,
        color: projects.color,
      })
      .from(projects)
      .where(
        and(
          inArray(
            projects.workspaceId,
            memberWorkspaces.map((w) => w.id)
          ),
          isNull(projects.archivedAt),
          isNull(projects.deletedAt)
        )
      )
      .orderBy(asc(projects.sortOrder), asc(projects.name))
    return {
      workspaces: memberWorkspaces.map((w) => ({
        ...w,
        projects: projectRows.filter((p) => p.workspaceId === w.id),
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
        allWorkspaces: z.boolean().default(false),
        workspaceIds: z.array(z.string().uuid()).max(500).default([]),
        projectIds: z.array(z.string().uuid()).max(2000).default([]),
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
        !input.allWorkspaces &&
        input.workspaceIds.length === 0 &&
        input.projectIds.length === 0
      ) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `Select at least one workspace or project, or deny access.`,
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
      const memberWorkspaceIds = new Set(
        (await getMemberWorkspaces(userId)).map((w) => w.id)
      )
      const workspaceIds = input.allWorkspaces
        ? []
        : input.workspaceIds.filter((id) => memberWorkspaceIds.has(id))
      let projectIds: string[] = []
      if (!input.allWorkspaces && input.projectIds.length > 0) {
        const rows = await db
          .select({ id: projects.id, workspaceId: projects.workspaceId })
          .from(projects)
          .where(
            and(
              inArray(projects.id, input.projectIds),
              isNull(projects.deletedAt)
            )
          )
        projectIds = rows
          .filter((p) => memberWorkspaceIds.has(p.workspaceId))
          .map((p) => p.id)
      }
      if (!input.allWorkspaces && workspaceIds.length === 0 && projectIds.length === 0) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `None of the selected workspaces/projects are accessible to your account.`,
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
          allWorkspaces: input.allWorkspaces,
          workspaceIds,
          projectIds,
        })
        .onConflictDoUpdate({
          target: [mcpGrants.userId, mcpGrants.clientId],
          set: {
            allWorkspaces: input.allWorkspaces,
            workspaceIds,
            projectIds,
            updatedAt: new Date(),
          },
        })

      return consent
    }),
})
