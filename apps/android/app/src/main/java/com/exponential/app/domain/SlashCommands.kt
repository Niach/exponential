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

    /** The rows applicable to [agent] (null = the default agent), in contract
     *  order. */
    fun catalogFor(agent: String?): List<SlashCommand> {
        val id = agent?.takeIf { it.isNotBlank() } ?: defaultAgent
        return all.filter { id in it.agents }
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
    fun matches(draft: String, agent: String?): List<SlashCommand> {
        if (!MENU_DRAFT.matches(draft)) return emptyList()
        val query = draft.drop(1)
        return catalogFor(agent).filter { it.name.startsWith(query, ignoreCase = true) }
    }

    /**
     * The catalog row [text] would run, if any: its first whitespace token
     * must equal `/name` (case-insensitive) for a row applicable to [agent].
     * Prose that merely mentions a command, and paths like `/api/foo`, are not
     * commands.
     */
    fun commandFor(text: String, agent: String?): SlashCommand? {
        val trimmed = text.trim()
        if (!trimmed.startsWith("/")) return null
        val head = trimmed.drop(1).takeWhile { !it.isWhitespace() }
        if (head.isEmpty()) return null
        return catalogFor(agent).firstOrNull { it.name.equals(head, ignoreCase = true) }
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
