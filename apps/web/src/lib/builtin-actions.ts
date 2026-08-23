// Built-in virtual actions (EXP-257/EXP-259). The server injects these into
// `actions.list` so every client renders them like team actions — but they
// are non-editable and non-deletable, and their prompt is composed by the
// DESKTOP from its own shipped constants, so the `body` here stays empty and
// is never fetched (EXP-268 removed the per-device trust gate entirely; these
// are product-shipped prompts, not team-owner ones). Two builtins exist
// today: "Create action", which runs the
// MCP-enabled action-creator prompt as a normal, steer-visible action
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

/** Reserved non-UUID id of the hidden "Chat" builtin (EXP-615). Unlike the
 * other two it is NEVER appended to `actions.list` or any picker — clients
 * reach it only through the start-coding dialog's Chat tab. */
export const BUILTIN_CHAT_ID = contract.builtinAction.chatId

export const BUILTIN_CHAT_NAME = `Chat`

export function isBuiltinActionId(id: string): boolean {
  return (
    id === BUILTIN_CREATE_ACTION_ID ||
    id === BUILTIN_FIX_CONFLICTS_ID ||
    id === BUILTIN_CHAT_ID
  )
}

/** The display-name snapshot a builtin's `coding_sessions` rows carry —
 * server constants, never client text (the row must outlive client renames). */
export function builtinActionName(id: string): string {
  return id === BUILTIN_FIX_CONFLICTS_ID
    ? BUILTIN_FIX_CONFLICTS_NAME
    : id === BUILTIN_CHAT_ID
      ? BUILTIN_CHAT_NAME
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
  {
    key: `name`,
    label: `Name`,
    type: `text`,
    required: false,
    placeholder: `Name (optional)`,
  },
  { key: `repo`, label: `Repository`, type: `repo`, required: false },
  // EXP-273: the user picks the new action's glyph up front and the
  // action-creator prompt passes it to `exponential_actions_create`, so a
  // team's action list is visually scannable from the moment it is authored.
  { key: `icon`, label: `Icon`, type: `icon`, required: false },
]

const FIX_CONFLICTS_INPUTS: ActionInputDef[] = [
  { key: `pr`, label: `Pull request`, type: `pr`, required: true },
]

const CHAT_INPUTS: ActionInputDef[] = [
  {
    key: `prompt`,
    label: `Prompt`,
    type: `textarea`,
    required: true,
    placeholder: `What should the agent do?`,
  },
  { key: `repo`, label: `Repository`, type: `repo`, required: true },
]

export interface BuiltinAction {
  id: string
  teamId: string
  repositoryId: null
  name: string
  description: string
  /** Curated registry icon name (EXP-273); null = the generic action glyph. */
  icon: string | null
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
    description: `Describe a new action and let your agent author it for the team`,
    icon: `sparkles`,
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
    description: `Pick a conflicted pull request and let your agent rebase, resolve, and merge it`,
    icon: `git-branch`,
    body: ``,
    inputs: FIX_CONFLICTS_INPUTS,
    sortOrder: 1e9 + 1,
    createdAt: new Date(0),
    updatedAt: new Date(0),
    builtin: true,
  }
}

/** The hidden "Chat" builtin (EXP-615): a free-prompt agent session on a
 * repository's trunk clone at the default branch. Deliberately appended to NO
 * list — the start-coding dialog's Chat tab constructs it directly. */
export function builtinChatAction(teamId: string): BuiltinAction {
  return {
    id: BUILTIN_CHAT_ID,
    teamId,
    repositoryId: null,
    name: BUILTIN_CHAT_NAME,
    description: `Chat with your agent on a repository`,
    icon: `message-circle`,
    body: ``,
    inputs: CHAT_INPUTS,
    sortOrder: 1e9 + 2,
    createdAt: new Date(0),
    updatedAt: new Date(0),
    builtin: true,
  }
}
