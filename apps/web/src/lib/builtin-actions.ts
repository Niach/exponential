// Built-in virtual actions (EXP-257/EXP-259). The server injects these into
// `actions.list` so every client renders them like team actions — but they
// are non-editable and non-deletable, and their prompt is composed by the
// DESKTOP from its own shipped constants (the `body` here stays empty and the
// trust gate is skipped: this is product-shipped content, not a team-owner
// prompt). Two builtins exist today: "Create action", which runs the
// MCP-enabled Claude action-creator prompt as a normal, steer-visible action
// run (it replaced every manual action-creation UI), and "Fix merge
// conflicts" (EXP-259), which takes a `pr` input (an issue-linked open PR),
// rebases its branch onto the default branch in a worktree, resolves the
// conflicts, pushes, and merges the PR via the `exponential_pr_merge` MCP
// tool.

import type { ActionInputDef } from "@exp/db-schema/domain"
import { contract } from "@exp/domain-contract"

/** Reserved non-UUID id — can never collide with a `uuid` PK. */
export const BUILTIN_CREATE_ACTION_ID = contract.builtinAction.createActionId

export const BUILTIN_CREATE_ACTION_NAME = `Create action`

/** Reserved non-UUID id of the "Fix merge conflicts" builtin (EXP-259). */
export const BUILTIN_FIX_CONFLICTS_ID = contract.builtinAction.fixConflictsId

export const BUILTIN_FIX_CONFLICTS_NAME = `Fix merge conflicts`

export function isBuiltinActionId(id: string): boolean {
  return id === BUILTIN_CREATE_ACTION_ID || id === BUILTIN_FIX_CONFLICTS_ID
}

/** The display-name snapshot a builtin's `coding_sessions` rows carry —
 * server constants, never client text (the row must outlive client renames). */
export function builtinActionName(id: string): string {
  return id === BUILTIN_FIX_CONFLICTS_ID
    ? BUILTIN_FIX_CONFLICTS_NAME
    : BUILTIN_CREATE_ACTION_NAME
}

const CREATE_ACTION_INPUTS: ActionInputDef[] = [
  {
    key: `description`,
    label: `Description`,
    type: `text`,
    required: true,
    placeholder: `What should this action do?`,
  },
  { key: `repo`, label: `Repository`, type: `repo`, required: false },
]

const FIX_CONFLICTS_INPUTS: ActionInputDef[] = [
  { key: `pr`, label: `Pull request`, type: `pr`, required: true },
]

export interface BuiltinAction {
  id: string
  teamId: string
  repositoryId: null
  name: string
  description: string
  body: string
  inputs: ActionInputDef[]
  sortOrder: number
  createdAt: Date
  updatedAt: Date
  builtin: true
}

/** The virtual "Create action" row appended to `actions.list`. Clients pin it
 * FIRST by the `builtin` flag (the huge sortOrder only keeps naive
 * sortOrder-asc renderers from interleaving it with real actions). */
export function builtinCreateAction(teamId: string): BuiltinAction {
  return {
    id: BUILTIN_CREATE_ACTION_ID,
    teamId,
    repositoryId: null,
    name: BUILTIN_CREATE_ACTION_NAME,
    description: `Describe a new action and let Claude author it for the team`,
    body: ``,
    inputs: CREATE_ACTION_INPUTS,
    sortOrder: 1e9,
    createdAt: new Date(0),
    updatedAt: new Date(0),
    builtin: true,
  }
}

/** The virtual "Fix merge conflicts" row appended to `actions.list`
 * (EXP-259). Its `pr` input is the representative issue id of an open PR;
 * the desktop rebases that PR's branch onto the default branch, resolves the
 * conflicts, pushes, and merges via `exponential_pr_merge`. */
export function builtinFixConflictsAction(teamId: string): BuiltinAction {
  return {
    id: BUILTIN_FIX_CONFLICTS_ID,
    teamId,
    repositoryId: null,
    name: BUILTIN_FIX_CONFLICTS_NAME,
    description: `Pick a conflicted pull request and let Claude rebase, resolve, and merge it`,
    body: ``,
    inputs: FIX_CONFLICTS_INPUTS,
    sortOrder: 1e9 + 1,
    createdAt: new Date(0),
    updatedAt: new Date(0),
    builtin: true,
  }
}
