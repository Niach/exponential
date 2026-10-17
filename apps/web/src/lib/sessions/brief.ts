import { TRPCError } from "@trpc/server"
import { and, asc, eq } from "drizzle-orm"
import { composeBrief, type BriefInput } from "@exp/domain-contract/session-brief"
import { isPersistent } from "@exp/domain-contract/session-kind"
import { db } from "@/db/connection"
import { actions, issues, messages, sessionIssues, sessions, teams, type Session } from "@/db/schema"
import { resolveActionInputs } from "@/lib/action-inputs"
import { actionInputLookups } from "@/lib/sessions/action-lookups"
import { deriveSessionTitle } from "@/lib/sessions/title"

export { deriveSessionTitle }

// EXP-1291: the server-composed FIRST prompt of a new session
// (`sessions.brief`, fixture-locked by packages/domain-contract
// session-brief.json) plus the system-append layers the device stacks under
// its runbook: the team prompt and, in an agent's tree, the agent's
// server-only instructions.

async function firstPersonMessage(sessionId: string): Promise<string | null> {
  const [row] = await db
    .select({ body: messages.body })
    .from(messages)
    .where(and(eq(messages.sessionId, sessionId), eq(messages.authorKind, `user`)))
    .orderBy(asc(messages.createdAt))
    .limit(1)
  return row?.body ?? null
}

async function sessionIssueRows(session: Pick<Session, `id` | `rootId`>) {
  // EXP-1341: a child's Issues = the ones ASSIGNED to it, else the root's.
  const fields = { identifier: issues.identifier, title: issues.title }
  const order = [asc(sessionIssues.sortOrder), asc(sessionIssues.createdAt)] as const
  if (session.rootId !== session.id) {
    const assigned = await db
      .select(fields)
      .from(sessionIssues)
      .innerJoin(issues, eq(issues.id, sessionIssues.issueId))
      .where(
        and(
          eq(sessionIssues.sessionId, session.rootId),
          eq(sessionIssues.assignedSessionId, session.id)
        )
      )
      .orderBy(...order)
    if (assigned.length > 0) return assigned
  }
  return db
    .select(fields)
    .from(sessionIssues)
    .innerJoin(issues, eq(issues.id, sessionIssues.issueId))
    .where(eq(sessionIssues.sessionId, session.rootId))
    .orderBy(...order)
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

  const [parent] = session.parentId
    ? await db.select().from(sessions).where(eq(sessions.id, session.parentId)).limit(1)
    : []
  // A dispatcher hands its action on: the work child runs it.
  const actionSource = session.actionId ? session : parent?.dispatch ? parent : null
  if (actionSource?.actionId) {
    const [action] = await db
      .select({ name: actions.name, body: actions.body, inputs: actions.inputs })
      .from(actions)
      .where(eq(actions.id, actionSource.actionId))
      .limit(1)
    if (action) {
      const resolved = await resolveActionInputs(
        action.inputs ?? [],
        actionSource.actionInputs ?? {},
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
              .filter((def) => actionSource.actionInputs?.[def.key])
              .map((def) => ({ label: def.label, display: actionSource.actionInputs![def.key]! })),
      }
    }
  }

  input.issues = await sessionIssueRows(session)
  // A thread's dispatcher started on work by construction hands it off at
  // once instead of judging it: plan mode, linked issues or an action. An
  // ended dispatcher's continuation (a child row that dispatches on) too.
  input.handOff =
    session.dispatch &&
    !isPersistent(session) &&
    Boolean(session.workLaunch?.planMode || input.issues.length > 0 || input.action)
  // EXP-1280: plan mode is Exponential's (a brief section + the pr_open
  // gate), never a dispatcher's: it hands plan mode on to its children.
  input.planMode = session.planMode === true && !session.dispatch

  let parentBranch: string | null = null
  if (session.parentId) {
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
