// S-3 (owner 2026-10-28): the New-chat composer's AGENT ROW. Pure and locked
// by `fixtures/composer-home.json` `agentRow.cases`, replayed by desktop
// `domain::composer_agent_row`, iOS ExpCore `ComposerAgentRow.swift` and
// Android `domain/ComposerAgentRow.kt`. The fixture's `$comment` holds the
// rules; this is the reference.

import fixture from "../fixtures/composer-home.json" with { type: "json" }
import { orderedAgents, type AgentRoleLike } from "./agent-settings"

export const COMPOSER_AGENT_ROW = fixture.agentRow
/** The dotted circle's id (Temporary session = a normal session). */
export const TEMPORARY_PICK: string = fixture.agentRow.temporaryId

export interface ComposerAgentRowAgent extends AgentRoleLike {
  id: string
  name: string
  sortOrder: number
}

export interface ComposerAgentRowState<T extends ComposerAgentRowAgent> {
  /** The circles in order: the agents, then the dotted circle. */
  agents: T[]
  /** Option ids in order, `TEMPORARY_PICK` last. */
  options: string[]
  /** The selected option id (never empty). */
  selected: string
  /** The picked agent, null = Temporary session. */
  agent: T | null
  /** The label line over the circles. */
  label: string
}

/** The row for a viewer: `picked` = the last tap (null/unknown = the default). */
export function composerAgentRow<T extends ComposerAgentRowAgent>(input: {
  agents: T[]
  me: string | null | undefined
  picked: string | null | undefined
}): ComposerAgentRowState<T> {
  const agents = orderedAgents(input.agents, input.me)
  const options = [...agents.map((agent) => agent.id), TEMPORARY_PICK]
  const fallback = agents.find((agent) => agent.role === `assistant`)?.id ?? TEMPORARY_PICK
  const selected = input.picked && options.includes(input.picked) ? input.picked : fallback
  const agent = agents.find((candidate) => candidate.id === selected) ?? null
  const label = agent ? fixture.agentRow.copy.to.replace(`{name}`, () => agent.name) : fixture.agentRow.copy.temporary
  return { agents, options, selected, agent, label }
}
