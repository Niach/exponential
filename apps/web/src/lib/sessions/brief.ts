import { TRPCError } from "@trpc/server"
import { and, asc, eq } from "drizzle-orm"
import { composeBrief, type BriefInput } from "@exp/domain-contract/session-brief"
import { db } from "@/db/connection"
import { actions, issues, messages, sessionIssues, sessions, teams, type Session } from "@/db/schema"
import { resolveActionInputs } from "@/lib/action-inputs"
import { actionInputLookups } from "@/lib/sessions/action-lookups"
import { deriveSessionTitle } from "@/lib/sessions/title"

export { deriveSessionTitle }

// EXP-1291/NEW-1: the server-composed FIRST prompt of a new session
// (`sessions.brief`, fixture-locked by packages/domain-contract
// session-brief.json) plus the system-append layers the device stacks under
// its runbook: the team prompt and, in an agent's tree, the agent's
// server-only instructions. A session's Issues = ITS OWN rows (root or
// child alike); a root works them itself.

async function firstPersonMessage(sessionId: string): Promise<string | null> {
  const [row] = await db
    .select({ body: messages.body })
    .from(messages)
    .where(and(eq(messages.sessionId, sessionId), eq(messages.authorKind, `user`)))
    .orderBy(asc(messages.createdAt))
    .limit(1)
  return row?.body ?? null
}

async function sessionIssueRows(session: Pick<Session, `id`>) {
  return db
    .select({ identifier: issues.identifier, title: issues.title })
    .from(sessionIssues)
    .innerJoin(issues, eq(issues.id, sessionIssues.issueId))
    .where(eq(sessionIssues.sessionId, session.id))
    .orderBy(asc(sessionIssues.sortOrder), asc(sessionIssues.createdAt))
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

  input.issues = await sessionIssueRows(session)
  // EXP-1280: plan mode is Exponential's (a brief section + the pr_open gate).
  input.planMode = session.planMode === true

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
    agentInstructions: root.instructions?.trim() || null,
    parentBranch,
  }
}
