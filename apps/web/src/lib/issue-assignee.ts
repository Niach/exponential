// #EXP-1383 (agent-settings.json `assignee`): an issue's assignee is ONE of
// two mutually exclusive columns, a member (`assigneeId`) or a team agent
// (`assigneeAgentId`). Every client write sends BOTH, the cleared one as an
// explicit null, so picking an agent unassigns the person and vice versa.

export interface IssueAssignee {
  assigneeId: string | null
  assigneeAgentId: string | null
}

export const UNASSIGNED: IssueAssignee = { assigneeId: null, assigneeAgentId: null }

/** A row's assignee pair (a draft or an issue), absent columns as null. */
export function issueAssignee(row: {
  assigneeId?: string | null
  assigneeAgentId?: string | null
}): IssueAssignee {
  return { assigneeId: row.assigneeId ?? null, assigneeAgentId: row.assigneeAgentId ?? null }
}

/** The two-column write: an agent wins and clears the person. */
export function assigneeWrite(next: IssueAssignee): IssueAssignee {
  return next.assigneeAgentId
    ? { assigneeId: null, assigneeAgentId: next.assigneeAgentId }
    : { assigneeId: next.assigneeId, assigneeAgentId: null }
}
