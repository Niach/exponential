package com.exponential.app.domain

// EXP-724: the curated steer slash-command catalog, read straight off the
// generated domain contract (`packages/domain-contract/contract.json`
// `steerCommands`) — the SAME rows every viewer's `/` menu offers, so what a
// client can pick, the desktop publisher can execute.
//
// Commands ride the ORDINARY composer path: the picked text is sent as a
// normal steer message (`/compact …` + the usual `\r` frame) and the desktop
// recognises it by its first token. Nothing here touches the wire.
//
// Mirrors `steer::commands` (desktop), `lib/steer-commands.ts` (web) and
// ExpCore's `SlashCommands.swift` (iOS) — the matching rules below are the
// same on all four and are locked by SlashCommandsTest.

/** One catalog row (a zipped view over the contract's parallel arrays). */
data class SlashCommand(
    val name: String,
    val description: String,
    /** Empty = the command takes no argument. */
    val argHint: String,
    /** The `coding_agent` values this command applies to. */
    val agents: List<String>,
    /** The client confirms before sending (context is discarded). */
    val confirm: Boolean,
) {
    /** What accepting the row puts in the composer: a trailing space only when
     *  there is an argument to type. Never auto-sends. */
    val insertion: String get() = if (argHint.isEmpty()) "/$name" else "/$name "
}

object SlashCommands {

    /** Every row, agent-agnostic, in contract order. */
    val all: List<SlashCommand> = DomainContract.steerCommandNames.indices.map { i ->
        SlashCommand(
            name = DomainContract.steerCommandNames[i],
            description = DomainContract.steerCommandDescriptions[i],
            argHint = DomainContract.steerCommandArgHints[i],
            agents = DomainContract.steerCommandAgents[i].split(",").filter { it.isNotBlank() },
            confirm = DomainContract.steerCommandConfirm[i],
        )
    }

    /** A session with no recorded agent ran on the default one. */
    private val defaultAgent: String = DomainContract.codingAgentValues.first()

    /**
     * EXP-746: the catalog id of an EXTERNAL ACP agent — deliberately a value
     * contract `codingAgent` (and so `steerCommands`) cannot name, so its
     * curated catalog is EMPTY, exactly like the desktop's
     * `steer::commands::agent_id(SessionAgent::External)`.
     */
    const val EXTERNAL_AGENT: String = "external"

    /**
     * EXP-746: which catalog a session's `/` menu and command rows key off.
     *
     * A row that names no agent is normally a claude run that predates the
     * column — but an EXTERNAL agent syncs no agent EITHER, because
     * `coding_sessions.agent` takes contract values only and there is none for
     * one. [acp] tells them apart: only the ACP engine publishes a
     * `config_state`, and an ACP run for claude/codex always stamps its id.
     * Without this the phone offered `/compact` and `/clear` — confirm dialog
     * and all — for a run whose desktop-side catalog is empty, and the literal
     * text reached the agent as a prompt. Mirrored ×4 (web `steerAgentId`, iOS
     * `SlashCommands.agentId`, desktop `slash_commands::agent_of`).
     */
    fun agentId(agent: String?, acp: Boolean): String =
        agent?.takeIf { it.isNotBlank() }?.trim()
            ?: if (acp) EXTERNAL_AGENT else defaultAgent

    /** The rows applicable to [agent] (null = the default agent), in contract
     *  order. */
    fun catalogFor(agent: String?): List<SlashCommand> {
        val id = agent?.takeIf { it.isNotBlank() } ?: defaultAgent
        return all.filter { id in it.agents }
    }

    /**
     * EXP-746: the `/` menu's catalog for a live ACP run — the CONTRACT rows
     * first, then the commands the agent itself advertised in `config_state`,
     * deduped by lowercased name (a contract row always wins: the desktop can
     * execute it, and the agent's own twin would shadow that). Byte-identical
     * ordering ×4 (`mergeAgentCommands` on web, iOS/desktop `merged`).
     *
     * An agent row carries no [SlashCommand.agents] filter: it came from THIS
     * run's agent, and the list handed in is already agent-scoped — nothing
     * re-filters a merged catalog.
     */
    fun merged(contract: List<SlashCommand>, agent: List<ConfigCommand>): List<SlashCommand> {
        if (agent.isEmpty()) return contract
        val seen = contract.mapTo(mutableSetOf()) { it.name.lowercase() }
        return contract + agent.mapNotNull { command ->
            // The wire form may or may not carry the sigil; the catalog never
            // does (its `insertion` adds it).
            val name = command.name.removePrefix("/").trim()
            if (name.isEmpty() || !seen.add(name.lowercase())) {
                null
            } else {
                SlashCommand(
                    name = name,
                    description = command.description,
                    argHint = command.hint.orEmpty(),
                    agents = emptyList(),
                    // Only the contract knows which commands discard context.
                    confirm = false,
                )
            }
        }
    }

    /**
     * The rows the `/` menu should offer for the CURRENT draft, or an empty
     * list when the menu must stay shut.
     *
     * The menu opens only while the WHOLE draft is a command being typed —
     * a leading `/` at position 0 and no whitespace yet — so `/` inside prose
     * or a path never pops it. The filter is a case-insensitive name PREFIX;
     * a bare `/` offers every row for the agent.
     */
    fun matches(
        draft: String,
        agent: String?,
        /** EXP-746: the run's own advertised commands, appended after the
         *  contract rows. Empty on a PTY run, which advertises none. */
        agentCommands: List<ConfigCommand> = emptyList(),
    ): List<SlashCommand> {
        if (!MENU_DRAFT.matches(draft)) return emptyList()
        val query = draft.drop(1)
        return merged(catalogFor(agent), agentCommands)
            .filter { it.name.startsWith(query, ignoreCase = true) }
    }

    /**
     * The catalog row [text] would run, if any: its first whitespace token
     * must equal `/name` (case-insensitive) for a row applicable to [agent].
     * Prose that merely mentions a command, and paths like `/api/foo`, are not
     * commands.
     */
    fun commandFor(
        text: String,
        agent: String?,
        agentCommands: List<ConfigCommand> = emptyList(),
    ): SlashCommand? {
        val trimmed = text.trim()
        if (!trimmed.startsWith("/")) return null
        val head = trimmed.drop(1).takeWhile { !it.isWhitespace() }
        if (head.isEmpty()) return null
        return merged(catalogFor(agent), agentCommands)
            .firstOrNull { it.name.equals(head, ignoreCase = true) }
    }

    /** The confirm dialog's title — byte-identical ×4. */
    fun confirmTitle(name: String): String = "Run /$name?"

    /** The confirm dialog's body — byte-identical ×4. */
    const val CONFIRM_BODY: String =
        "The agent forgets everything in this session so far. " +
            "Files in the worktree are kept."

    /** The confirm dialog's action — byte-identical ×4. */
    fun confirmButton(name: String): String = "Run /$name"

    /** The whole draft is a command being typed: `/`, then name characters
     *  only. The first space closes the menu (the argument is being typed). */
    private val MENU_DRAFT = Regex("^/[A-Za-z0-9-]*$")
}
