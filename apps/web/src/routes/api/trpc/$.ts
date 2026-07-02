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
import { repositoriesRouter } from "@/lib/trpc/repositories"
import { runConfigsRouter } from "@/lib/trpc/run-configs"
import { codingSessionsRouter } from "@/lib/trpc/coding-sessions"
import { steerRouter } from "@/lib/trpc/steer"
import { billingRouter } from "@/lib/trpc/billing"
import { onboardingRouter } from "@/lib/trpc/onboarding"
import { subscriptionsRouter } from "@/lib/trpc/subscriptions"
import { notificationsRouter } from "@/lib/trpc/notifications"
import { widgetsRouter } from "@/lib/trpc/widgets"

export const appRouter = router({
  workspaces: workspacesRouter,
  projects: projectsRouter,
  issues: issuesRouter,
  issueLabels: issueLabelsRouter,
  labels: labelsRouter,
  comments: commentsRouter,
  repositories: repositoriesRouter,
  runConfigs: runConfigsRouter,
  codingSessions: codingSessionsRouter,
  steer: steerRouter,
  workspaceInvites: workspaceInvitesRouter,
  workspaceMembers: workspaceMembersRouter,
  users: usersRouter,
  integrations: integrationsRouter,
  admin: adminRouter,
  pushTokens: pushTokensRouter,
  billing: billingRouter,
  onboarding: onboardingRouter,
  subscriptions: subscriptionsRouter,
  notifications: notificationsRouter,
  widgets: widgetsRouter,
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
