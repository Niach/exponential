import { z } from "zod"
import { eq } from "drizzle-orm"
import { authedProcedure } from "@/lib/trpc"
import { workspaceAgents } from "@/db/schema"
import { encryptSecret } from "@/lib/crypto/secret-box"
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
        // The agent's GitHub token, stored encrypted so the server can read PR
        // diffs for private repos. Optional — only overwrites when provided.
        token: z.string().min(1).max(512).optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const agent = await loadAgentForSessionUser(ctx.db, ctx.session.user.id)
      const set: Record<string, unknown> = {
        githubUserLogin: input.login,
        githubRepos: input.repos,
        lastSeenAt: new Date(),
      }
      if (input.token) set.githubToken = encryptSecret(input.token)
      await ctx.db
        .update(workspaceAgents)
        .set(set)
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
        githubToken: null,
        lastSeenAt: new Date(),
      })
      .where(eq(workspaceAgents.id, agent.id))
    return { ok: true }
  }),
}
