import { McpServer } from "@modelcontextprotocol/server"
import { users } from "@/db/auth-schema"
import { registerNounTools } from "./nouns"
import { registerExponentialApps } from "./apps"
import { registerIssueResource } from "./mentions"
import { MCP_EXTENSION_CAPABILITIES } from "./extensions"
import { NO_CLIENT, type EmcpContext } from "./request-context"
import { requestStateVerifier } from "./mrtr"
import { registerSkills } from "./skills-mcp"
import { registerTasks } from "./tasks"
import { appBaseUrl } from "@/lib/notification-email-policy"
import { mcpServerInstructions } from "./instructions"
import { buildRuntimeConfig } from "@/lib/runtime-config"
import type { McpAccess } from "./scope"
import { ALL_MCP_TOOL_GATES, type McpToolGates } from "./gates"

export type McpUser = typeof users.$inferSelect

// `sessionId` (EXP-637) is the `sessions` row this request runs inside,
// parsed from the launcher-injected X-Exp-Session-Id header. Null for every
// caller that is not a launched agent (a human's MCP client, a bare api key);
// tools that need it say so instead of guessing. `gates` (EXP-660) is what
// the route resolved for this caller (resolveMcpToolGates); the default is
// the full surface. `context` (#NEW-144) is what the gate read of this
// request's client and credential: every handler gets it as `ctx.client` /
// `ctx.principal`.
export function createExponentialMcpServer(
  user: McpUser,
  request: Request,
  access: McpAccess,
  sessionId: string | null = null,
  gates: McpToolGates = ALL_MCP_TOOL_GATES,
  context: EmcpContext = { client: NO_CLIENT, principal: { userId: user.id, grant: `member` } }
) {
  const server = new McpServer(
    {
      name: `exponential`,
      version: `0.1.0`,
    },
    // Loaded up front by every client even when tool definitions are
    // deferred behind tool search — see instructions.ts for the byte budget.
    // EXP-679 / EXP-1222: the guidance follows the gates, so the close-out
    // wording matches who is watching the run. FEED-21: reportBug mirrors
    // the EXP-496 registration check in tools.ts exactly.
    {
      ...(sessionId ? {} : { capabilities: MCP_EXTENSION_CAPABILITIES }),
      // #NEW-144: an MRTR retry's requestState is opened, checked against
      // this request's member, credential and call, and spent before any
      // handler runs (mrtr.ts).
      requestState: { verify: requestStateVerifier(context) },
      // A 2025 request never gets input_required (askInput answers null):
      // the SDK's legacy shim would need a server→client channel the
      // stateless JSON transport does not have.
      inputRequired: { legacyShim: false },
      instructions: mcpServerInstructions({
        inSession: gates.inSession,
        unattended: gates.unattended,
        agent: gates.agent,
        reportBug: Boolean(buildRuntimeConfig().feedbackWidget?.widgetKey),
      }),
    }
  )
  // NEW-4: the 17 tools (S-3 added `show`) route `op` to the per-operation definitions.
  const tools = registerNounTools(server, user, request, access, sessionId, gates, context)
  // EXP-1183: the MCP Apps views the issue/run tools point at. EXP-1212: not
  // for a coding run (it cannot draw them, and reading one would pull the
  // whole inlined page into its context); external MCP Apps hosts only.
  if (!sessionId) {
    registerIssueResource(server, tools)
    registerExponentialApps(
      server,
      process.env.BETTER_AUTH_URL ? appBaseUrl() : new URL(request.url).origin
    )
    // #NEW-147: skills over MCP (skills.json) and tasks (tasks.json) for
    // external hosts only; extensions.ts declares both.
    registerSkills(server, user.id, access)
    registerTasks(server, context)
  }
  return server
}
