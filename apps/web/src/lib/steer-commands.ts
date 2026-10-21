import { contract } from "@exp/domain-contract"

// EXP-724 — the curated slash commands a viewer may send into a live steering
// session. Pure: the `/` menu (components/steer-command-menu.tsx), the
// composer's send path and the feed's command pill all read these helpers, and
// the three native viewers mirror them rule for rule (iOS SlashCommands.swift,
// Android SlashCommands.kt, desktop ui/src/slash_commands.rs).
//
// The catalog itself is contract data (packages/domain-contract/contract.json
// `steerCommands`: compact, model, effort), so the list a phone offers is
// byte-identical to the one the desktop publisher knows how to execute.
// NEW-2: commands the agent CLI injects (`config_state.commands`: Claude Code
// skills, plugins) are HIDDEN on every client — never merged into the menu;
// the team's PROMPTS follow the builtins instead (steer-command-menu.tsx).
// Commands ride the ORDINARY input frames as text, except `/effort`
// (`sessions.configure`, hooks/use-steer-command.ts).

export interface SteerCommand {
  /** The bare name, no leading slash (`compact`). */
  name: string
  description: string
  /** Placeholder for the command's argument, `` when it takes none. */
  argHint: string
  /** The coding agents that can run it (contract `codingAgent` ids). */
  agents: readonly string[]
  /** Sending it discards the conversation — confirm before it goes out. */
  confirm: boolean
}

/** A session whose synced row names no agent predates the column: it is a
 *  claude run (contract order puts claude first). */
export const DEFAULT_STEER_AGENT: string = contract.codingAgent.values[0]

/** EXP-746: the catalog id of an EXTERNAL ACP agent — deliberately a value
 *  contract `codingAgent` (and therefore `steerCommands`) cannot name, so its
 *  curated catalog is EMPTY. Mirrors the desktop's
 *  `steer::commands::agent_id(SessionAgent::External)`. */
export const EXTERNAL_STEER_AGENT = `external`

/** EXP-746: which catalog a session's `/` menu and command pills key off.
 *
 *  A row that names no agent is normally a claude run that predates the
 *  column — but an EXTERNAL ACP agent syncs no agent EITHER, because
 *  `coding_sessions.agent` takes contract values only and there is none for
 *  one. The two are told apart by `config_state`: only the ACP engine
 *  publishes it (every PTY run publishes none), and an ACP run for
 *  claude/codex always stamps its id — so an agent-less run that published
 *  one is external, and it gets the contract-less id above.
 *
 *  Without this, a phone offered `/compact` and `/clear` (confirm dialog and
 *  all) for a run whose desktop-side catalog is empty: the literal text
 *  reached the agent as a prompt and nothing was cleared. Mirrored ×4
 *  (iOS `SlashCommands.agentId(_:acp:)`, Android `SlashCommands.agentId`,
 *  desktop `slash_commands::agent_of`). */
export function steerAgentId(
  agent: string | null | undefined,
  acp: boolean
): string {
  const id = agent?.trim()
  if (id) return id
  return acp ? EXTERNAL_STEER_AGENT : DEFAULT_STEER_AGENT
}

/** The whole catalog, in contract order. */
export const STEER_COMMANDS: readonly SteerCommand[] =
  contract.steerCommands.commands

/** The commands one session's agent can actually run, in catalog order. */
export function steerCommandsFor(
  agent: string | null | undefined
): SteerCommand[] {
  const id = agent?.trim() ? agent.trim() : DEFAULT_STEER_AGENT
  return STEER_COMMANDS.filter((command) => command.agents.includes(id))
}

/** The `/` menu opens only while the WHOLE draft is a bare command token:
 *  a leading slash at position 0 and nothing but name characters after it.
 *  The first space closes the menu for good (the rest of the line is the
 *  command's argument, or ordinary prose that merely started with a slash).
 *  Returns the typed query without its slash, or null when no menu is due. */
export function matchSlashDraft(draft: string): string | null {
  const match = /^\/([A-Za-z0-9-]*)$/.exec(draft)
  return match ? match[1] : null
}

/** Case-insensitive name-PREFIX filter, catalog order preserved. An empty
 *  query offers every command the agent has. */
export function filterSteerCommands(
  commands: readonly SteerCommand[],
  query: string,
  limit = 8
): SteerCommand[] {
  const needle = query.trim().toLowerCase()
  return commands
    .filter((command) => command.name.toLowerCase().startsWith(needle))
    .slice(0, limit)
}

/** NEW-2: a team prompt as the `/` menu lists it (the synced, body-less
 *  `prompts` row): its icon and name, after the builtins. */
export interface SlashPrompt {
  id: string
  name: string
  icon: string | null
}

/** The token a prompt answers to after the slash: its name, lowercased,
 *  whitespace runs as `-` (`Fix flaky tests` → `fix-flaky-tests`), so
 *  `/fix` and `/fix-fl` both find it. */
export function promptSlashName(name: string): string {
  return name.trim().toLowerCase().replace(/\s+/g, `-`)
}

/** Case-insensitive PREFIX filter over the prompts' slash names, the
 *  team's order preserved. An empty query offers every prompt. */
export function filterSlashPrompts<T extends SlashPrompt>(
  prompts: readonly T[],
  query: string,
  limit = 50
): T[] {
  const needle = query.trim().toLowerCase()
  return prompts
    .filter((prompt) => promptSlashName(prompt.name).startsWith(needle))
    .slice(0, limit)
}

export interface ParsedSteerCommand {
  command: SteerCommand
  /** Everything after the command token, trimmed (`` when there is none). */
  args: string
}

/** A sent message is a command iff its FIRST whitespace-delimited token is
 *  exactly `/name` (case-insensitive) for one of the agent's commands. A
 *  message that merely mentions `/compact` mid-sentence is prose. */
export function parseSteerCommand(
  text: string,
  commands: readonly SteerCommand[]
): ParsedSteerCommand | null {
  const trimmed = text.trim()
  if (!trimmed.startsWith(`/`)) return null
  const space = trimmed.search(/\s/)
  const token = (space === -1 ? trimmed : trimmed.slice(0, space)).toLowerCase()
  const command = commands.find((c) => `/${c.name.toLowerCase()}` === token)
  if (!command) return null
  return {
    command,
    args: space === -1 ? `` : trimmed.slice(space).trim(),
  }
}

/** What accepting a candidate leaves in the composer. A command that takes an
 *  argument gets its trailing space so the user types straight on; one that
 *  takes none is left bare. Accepting NEVER sends. */
export function steerCommandDraft(command: SteerCommand): string {
  return command.argHint ? `/${command.name} ` : `/${command.name}`
}

// ── Copy (byte-identical on web, iOS, Android and the desktop viewer) ────────

/** EXP-746: a chip whose value is blank — the CLI's own default. */
export const CONFIG_DEFAULT_VALUE_LABEL = `CLI default`

/** EXP-746: the mode chip's leading label. Every other chip takes its leading
 *  label from the wire, so only this synthetic one needs a constant. */
export const CONFIG_MODE_LABEL = `Mode`

/** The compaction strip's label while the agent is folding its context. */
export const COMPACTING_LABEL = `Compacting context…`
/** The persistent feed marker a finished compaction leaves behind. */
export const COMPACTED_LABEL = `Context compacted`

export interface SteerCommandConfirmCopy {
  title: string
  body: string
  confirm: string
  cancel: string
}

/** The confirmation a context-discarding command (`/clear`) shows
 *  before anything goes out. Client-side only: the publisher executes what it
 *  receives, so every viewer must ask first. */
export function steerCommandConfirmCopy(name: string): SteerCommandConfirmCopy {
  return {
    title: `Run /${name}?`,
    body: `The agent forgets everything in this session so far. Files in the worktree are kept.`,
    confirm: `Run /${name}`,
    cancel: `Cancel`,
  }
}
