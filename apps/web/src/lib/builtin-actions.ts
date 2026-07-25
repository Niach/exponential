// Built-in virtual actions (EXP-257). The server injects these into
// `actions.list` so every client renders them like team actions — but they
// are non-editable and non-deletable, and their prompt is composed by the
// DESKTOP from its own shipped constants (the `body` here stays empty and the
// trust gate is skipped: this is product-shipped content, not a team-owner
// prompt). One builtin exists today: "Create action", which runs the
// MCP-enabled Claude action-creator prompt as a normal, steer-visible action
// run — it replaced every manual action-creation UI.

import type { ActionInputDef } from "@exp/db-schema/domain"
import { contract } from "@exp/domain-contract"

/** Reserved non-UUID id — can never collide with a `uuid` PK. */
export const BUILTIN_CREATE_ACTION_ID = contract.builtinAction.createActionId

export const BUILTIN_CREATE_ACTION_NAME = `Create action`

export function isBuiltinActionId(id: string): boolean {
  return id === BUILTIN_CREATE_ACTION_ID
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
