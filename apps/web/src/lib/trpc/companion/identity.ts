import { z } from "zod"
import { and, eq } from "drizzle-orm"
import { TRPCError } from "@trpc/server"
import { authedProcedure } from "@/lib/trpc"
import { projects, workspaceMembers } from "@/db/schema"
import { resolveRepoInstallationToken } from "@/lib/integrations/github-app"
import { loadAgentForSessionUser } from "./shared"

export const identityProcedures = {
  // The desktop agent fetches a short-lived GitHub App installation token to
  // clone/push `repo`. Gated: the caller must be the agent of a workspace whose
  // project points at this repo. (Server-side PR creation + diff mint their
  // own tokens; this is only for the local git transport.)
  repoToken: authedProcedure
    .input(
      z.object({
        repo: z.string().regex(/^[A-Za-z0-9._-]+\/[A-Za-z0-9._-]+$/),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const agent = await loadAgentForSessionUser(ctx.db, ctx.session.user.id)
      // The device is a member of many workspaces; allow the repo if it backs a
      // project in any of them.
      const [hit] = await ctx.db
        .select({ id: projects.id })
        .from(projects)
        .innerJoin(
          workspaceMembers,
          eq(workspaceMembers.workspaceId, projects.workspaceId)
        )
        .where(
          and(
            eq(workspaceMembers.userId, agent.userId),
            eq(projects.githubRepo, input.repo)
          )
        )
        .limit(1)
      if (!hit) {
        throw new TRPCError({
          code: `FORBIDDEN`,
          message: `Repo ${input.repo} is not in any of this device's workspaces`,
        })
      }
      const token = await resolveRepoInstallationToken(input.repo)
      return { token: token ?? null }
    }),
}
