import { TRPCError } from "@trpc/server"
import { and, asc, eq } from "drizzle-orm"
import { composeBrief, type BriefInput } from "@exp/domain-contract/session-brief"
import { db } from "@/db/connection"
import { actions, issues, messages, sessionIssues, sessions, teams, type Session } from "@/db/schema"
import { resolveActionInputs } from "@/lib/action-inputs"
import { actionInputLookups } from "@/lib/sessions/action-lookups"

// EXP-1291: the server-composed FIRST prompt of a new session
// (`sessions.brief`, fixture-locked by packages/domain-contract
// session-brief.json) plus the system-append layers the device stacks under
// its runbook: the team prompt and, in an agent's tree, the agent's
// server-only instructions.

/** The first line of a message body, as a title fallback. */
function firstLine(body: string | null | undefined): string | null {
  const line = (body ?? ``)
    .split(`\n`)
    .map((part) => part.trim())
    .find((part) => part.length > 0 && !part.startsWith(`![`))
  return line ? line.replace(/^#+\s*/, ``).slice(0, 120) : null
}

async function firstPersonMessage(sessionId: string): Promise<string | null> {
  const [row] = await db
    .select({ body: messages.body })
    .from(messages)
    .where(and(eq(messages.sessionId, sessionId), eq(messages.authorKind, `user`)))
    .orderBy(asc(messages.createdAt))
    .limit(1)
  return row?.body ?? null
}

async function rootIssues(rootId: string) {
  return db
    .select({ identifier: issues.identifier, title: issues.title })
    .from(sessionIssues)
    .innerJoin(issues, eq(issues.id, sessionIssues.issueId))
    .where(eq(sessionIssues.sessionId, rootId))
    .orderBy(asc(sessionIssues.sortOrder), asc(sessionIssues.createdAt))
}

/** A session's display title, the ONE derivation the server uses (the
 *  clients' `buildThreadView` mirrors it): title ?? action name ?? agent
 *  name ?? the first issue's title ?? the first message's first line. */
export async function deriveSessionTitle(
  session: Pick<Session, `id` | `rootId` | `title` | `actionName` | `name`>
): Promise<string> {
  const direct = session.title?.trim() || session.actionName?.trim() || session.name?.trim()
  if (direct) return direct
  if (session.rootId === session.id) {
    const [issue] = await rootIssues(session.id)
    if (issue) return issue.title
  }
  return firstLine(await firstPersonMessage(session.id)) ?? `Untitled`
}

export async function composeSessionBrief(sessionId: string): Promise<{
  brief: string
  teamPrompt: string | null
  agentInstructions: string | null
  parentBranch: string | null
}> {
  const [session] = await db.select().from(sessions).where(eq(sessions.id, sessionId)).limit(1)
  if (!session) throw new TRPCError({ code: `NOT_FOUND`, message: `Session not found` })
  const [root] =
    session.rootId === session.id
      ? [session]
      : await db.select().from(sessions).where(eq(sessions.id, session.rootId)).limit(1)
  if (!root) throw new TRPCError({ code: `NOT_FOUND`, message: `Session not found` })

  const input: BriefInput = { text: await firstPersonMessage(session.id) }

  if (session.actionId) {
    const [action] = await db
      .select({ name: actions.name, body: actions.body, inputs: actions.inputs })
      .from(actions)
      .where(eq(actions.id, session.actionId))
      .limit(1)
    if (action) {
      const resolved = await resolveActionInputs(
        action.inputs ?? [],
        session.actionInputs ?? {},
        session.teamId,
        actionInputLookups(db)
      )
      input.action = {
        name: action.name,
        body: action.body,
        // A pick that stopped resolving (a merged PR, a trashed board) keeps
        // its raw id rather than failing the launch.
        inputs: resolved.ok
          ? resolved.inputs.map((entry) => ({ label: entry.label, display: entry.display }))
          : (action.inputs ?? [])
              .filter((def) => session.actionInputs?.[def.key])
              .map((def) => ({ label: def.label, display: session.actionInputs![def.key]! })),
      }
    }
  }

  input.issues = await rootIssues(root.id)

  let parentBranch: string | null = null
  if (session.parentId) {
    const [parent] = await db
      .select()
      .from(sessions)
      .where(eq(sessions.id, session.parentId))
      .limit(1)
    if (parent) {
      parentBranch = parent.branch ?? null
      input.parent = {
        title: await deriveSessionTitle(parent),
        depth: session.depth,
        branch: parentBranch,
      }
    }
  }

  const [team] = await db
    .select({ agentPrompt: teams.agentPrompt })
    .from(teams)
    .where(eq(teams.id, session.teamId))
    .limit(1)

  return {
    brief: composeBrief(input),
    teamPrompt: team?.agentPrompt?.trim() || null,
    agentInstructions: root.kind === `agent` ? root.instructions?.trim() || null : null,
    parentBranch,
  }
}
