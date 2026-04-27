import { createFileRoute } from "@tanstack/react-router"
import { fetchRequestHandler } from "@trpc/server/adapters/fetch"
import { router } from "@/lib/trpc"
import { db } from "@/db/connection"
import { auth } from "@/lib/auth"
import { workspacesRouter } from "@/lib/trpc/workspaces"
import { projectsRouter } from "@/lib/trpc/projects"
import { issuesRouter } from "@/lib/trpc/issues"
import { issueLabelsRouter } from "@/lib/trpc/issue-labels"
import { labelsRouter } from "@/lib/trpc/labels"
import { workspaceInvitesRouter } from "@/lib/trpc/workspace-invites"
import { workspaceMembersRouter } from "@/lib/trpc/workspace-members"
import { usersRouter } from "@/lib/trpc/users"
import { integrationsRouter } from "@/lib/trpc/integrations"

export const appRouter = router({
  workspaces: workspacesRouter,
  projects: projectsRouter,
  issues: issuesRouter,
  issueLabels: issueLabelsRouter,
  labels: labelsRouter,
  workspaceInvites: workspaceInvitesRouter,
  workspaceMembers: workspaceMembersRouter,
  users: usersRouter,
  integrations: integrationsRouter,
})

export type AppRouter = typeof appRouter

const serve = ({ request }: { request: Request }) => {
  return fetchRequestHandler({
    endpoint: `/api/trpc`,
    req: request,
    router: appRouter,
    createContext: async () => ({
      db,
      request,
      session: await auth.api.getSession({ headers: request.headers }),
    }),
  })
}

export const Route = createFileRoute(`/api/trpc/$`)({
  server: {
    handlers: {
      GET: serve,
      POST: serve,
    },
  },
})
