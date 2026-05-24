import { z } from "zod"
import { eq } from "drizzle-orm"
import { authedProcedure } from "@/lib/trpc"
import { workspaceAgents } from "@/db/schema"
import { loadAgentForSessionUser } from "./shared"

export const identityProcedures = {
  reportGithubIdentity: authedProcedure
    .input(
      z.object({
        login: z.string().min(1).max(128),
        repos: z
          .array(
            z.object({
              fullName: z
                .string()
                .regex(/^[A-Za-z0-9._-]+\/[A-Za-z0-9._-]+$/),
              defaultBranch: z.string().min(1).max(255),
              private: z.boolean(),
            })
          )
          .max(2000),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const agent = await loadAgentForSessionUser(ctx.db, ctx.session.user.id)
      await ctx.db
        .update(workspaceAgents)
        .set({
          githubUserLogin: input.login,
          githubRepos: input.repos,
          lastSeenAt: new Date(),
        })
        .where(eq(workspaceAgents.id, agent.id))
      return { ok: true, count: input.repos.length }
    }),

  clearGithubIdentity: authedProcedure.mutation(async ({ ctx }) => {
    const agent = await loadAgentForSessionUser(ctx.db, ctx.session.user.id)
    await ctx.db
      .update(workspaceAgents)
      .set({
        githubUserLogin: null,
        githubRepos: null,
        lastSeenAt: new Date(),
      })
      .where(eq(workspaceAgents.id, agent.id))
    return { ok: true }
  }),
}
