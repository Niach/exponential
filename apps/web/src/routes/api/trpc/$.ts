import { createFileRoute } from "@tanstack/react-router"
import { fetchRequestHandler } from "@trpc/server/adapters/fetch"
import { router } from "@/lib/trpc"
import { db } from "@/db/connection"
import { resolveSession } from "@/lib/auth/resolve-bearer"
import { checkClientVersion } from "@/lib/client-version"
import { teamsRouter } from "@/lib/trpc/teams"
import { boardsRouter } from "@/lib/trpc/boards"
import { issuesRouter } from "@/lib/trpc/issues"
import { issueLabelsRouter } from "@/lib/trpc/issue-labels"
import { labelsRouter } from "@/lib/trpc/labels"
import { teamInvitesRouter } from "@/lib/trpc/team-invites"
import { teamMembersRouter } from "@/lib/trpc/team-members"
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
import { helpdeskRouter } from "@/lib/trpc/helpdesk"
import { mcpGrantsRouter } from "@/lib/trpc/mcp-grants"

export const appRouter = router({
  teams: teamsRouter,
  boards: boardsRouter,
  issues: issuesRouter,
  issueLabels: issueLabelsRouter,
  labels: labelsRouter,
  comments: commentsRouter,
  repositories: repositoriesRouter,
  runConfigs: runConfigsRouter,
  codingSessions: codingSessionsRouter,
  steer: steerRouter,
  teamInvites: teamInvitesRouter,
  teamMembers: teamMembersRouter,
  users: usersRouter,
  integrations: integrationsRouter,
  admin: adminRouter,
  pushTokens: pushTokensRouter,
  billing: billingRouter,
  onboarding: onboardingRouter,
  subscriptions: subscriptionsRouter,
  notifications: notificationsRouter,
  widgets: widgetsRouter,
  helpdesk: helpdeskRouter,
  mcpGrants: mcpGrantsRouter,
})

export type AppRouter = typeof appRouter

const serve = ({ request }: { request: Request }) => {
  // Gate outdated native clients with a real HTTP 426 — a tRPC middleware
  // can only produce a 200-wrapped error envelope, which clients can't
  // reliably distinguish at their HTTP layer.
  const upgradeRequired = checkClientVersion(request)
  if (upgradeRequired) return upgradeRequired

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
