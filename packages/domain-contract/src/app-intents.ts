// #EXP-1640: Siri / Shortcuts / App Actions. Pure and locked by
// `fixtures/app-intents.json`, replayed by iOS ExpCore `AppIntentsModel.swift`
// and Android `domain/AppIntents.kt`. The fixture's `$comment` holds the
// rules; this is the reference.

import fixture from "../fixtures/app-intents.json" with { type: "json" }
import { orderedAgents, type AgentRoleLike } from "./agent-settings"

export const APP_INTENTS = fixture

/** A spoken or typed issue title, null = ask again. */
export function intentIssueTitle(text: string): string | null {
  const title = text.replace(/\s+/g, ` `).trim()
  if (!title) return null
  return Array.from(title).slice(0, fixture.titleMax).join(``)
}

/** The board a new issue lands on, null = no board. */
export function intentBoard(input: {
  boards: { id: string; sortOrder: number }[]
  lastBoardId: string | null
  pickedId: string | null
}): string | null {
  const ids = new Set(input.boards.map((board) => board.id))
  if (input.pickedId && ids.has(input.pickedId)) return input.pickedId
  if (input.lastBoardId && ids.has(input.lastBoardId)) return input.lastBoardId
  return [...input.boards].sort((a, b) => a.sortOrder - b.sortOrder)[0]?.id ?? null
}

/** The agent a spoken name means: exact, then prefix, in agent order. */
export function matchIntentAgent<T extends AgentRoleLike & { id: string; name: string; sortOrder: number }>(input: {
  agents: T[]
  me: string | null | undefined
  spoken: string
}): string | null {
  const agents = orderedAgents(input.agents, input.me)
  const spoken = input.spoken.trim().toLowerCase()
  if (!spoken) return agents[0]?.id ?? null
  const exact = agents.find((agent) => agent.name.toLowerCase() === spoken)
  if (exact) return exact.id
  return agents.find((agent) => agent.name.toLowerCase().startsWith(spoken))?.id ?? null
}
