import { createFileRoute } from "@tanstack/react-router"
import { fetchRequestHandler } from "@trpc/server/adapters/fetch"
import { router } from "@/lib/trpc"
import { db } from "@/db/connection"
import { resolveSession } from "@/lib/auth/resolve-bearer"
import { workspacesRouter } from "@/lib/trpc/workspaces"
import { projectsRouter } from "@/lib/trpc/projects"
import { issuesRouter } from "@/lib/trpc/issues"
import { issueLabelsRouter } from "@/lib/trpc/issue-labels"
import { labelsRouter } from "@/lib/trpc/labels"
import { workspaceInvitesRouter } from "@/lib/trpc/workspace-invites"
import { workspaceMembersRouter } from "@/lib/trpc/workspace-members"
import { usersRouter } from "@/lib/trpc/users"
import { integrationsRouter } from "@/lib/trpc/integrations"
import { adminRouter } from "@/lib/trpc/admin"
import { pushTokensRouter } from "@/lib/trpc/push-tokens"
import { commentsRouter } from "@/lib/trpc/comments"
import { companionRouter } from "@/lib/trpc/companion"
import { agentPlanRouter } from "@/lib/trpc/agent-plan"
import { billingRouter } from "@/lib/trpc/billing"
import { onboardingRouter } from "@/lib/trpc/onboarding"
import { subscriptionsRouter } from "@/lib/trpc/subscriptions"
import { notificationsRouter } from "@/lib/trpc/notifications"

export const appRouter = router({
  workspaces: workspacesRouter,
  projects: projectsRouter,
  issues: issuesRouter,
  issueLabels: issueLabelsRouter,
  labels: labelsRouter,
  comments: commentsRouter,
  workspaceInvites: workspaceInvitesRouter,
  workspaceMembers: workspaceMembersRouter,
  users: usersRouter,
  integrations: integrationsRouter,
  admin: adminRouter,
  pushTokens: pushTokensRouter,
  // The desktop agent runtime + web agent UI call these under `agent.*` (the
  // companion daemon was replaced by agent-core). `companion` is a temporary
  // alias so already-deployed agent builds keep working for one release; drop it
  // once every client ships the `agent.*` paths.
  agent: companionRouter,
  companion: companionRouter,
  agentPlan: agentPlanRouter,
  billing: billingRouter,
  onboarding: onboardingRouter,
  subscriptions: subscriptionsRouter,
  notifications: notificationsRouter,
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
      session: await resolveSession(request),
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
