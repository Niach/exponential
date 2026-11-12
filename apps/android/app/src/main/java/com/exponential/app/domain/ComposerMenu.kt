package com.exponential.app.domain

// EXP-1249: the composer's ONE "+" menu as a pure layout, the x4 mirror of web
// `components/launch-dialog/composer-menu.ts`, locked against
// `packages/domain-contract/fixtures/composer-menu.json` (`ComposerMenuTest`):
// which rows show for the picked agent / device / subject, in what order,
// with which concept glyph and copy. Phones open it as a bottom sheet of the
// same rows, submenus as pushed pages (`AgentComposer.kt` binds the ids).
// EXP-1259: the suggestion rows are gone (`composer-home.json` owns the page
// around the card). NEW-2: the Run action row is gone (prompts live in the
// `/` menu); the `mcp` condition = the team has a server. S-3: rows carry
// `surfaces` (launch | session | agent) and `when` lists; Device / Account /
// Model / Repository are + rows (no options line); `message-agent` is gone.

enum class ComposerMenuRowId(val wire: String) {
    AddFile("add-file"),
    ImplementIssue("implement-issue"),
    /** #EXP-1434: the team's prompts (the `/` menu's list) as a submenu. */
    Prompts("prompts"),
    /** #EXP-1715: the viewer's Assistant's Teams pick (`teamsPick`), multi. */
    Teams("teams"),
    Device("device"),
    Account("account"),
    Model("model"),
    Repository("repository"),
    Effort("effort"),
    Ultracode("ultracode"),
    McpServers("mcp-servers"),
    /** #EXP-1661: the team's skills (skills.json `menuRow`), multi pick. */
    Skills("skills"),
    ComputerUse("computer-use"),
}

enum class ComposerMenuRowKind { Submenu, Item, Toggle }

/** S-3: which composer the "+" sits on (a row's `surfaces`). */
enum class ComposerMenuSurface(val wire: String) { Launch("launch"), Session("session"), Agent("agent") }

/** When a row shows (fixture `conditions`). */
enum class ComposerMenuCondition(val wire: String) {
    Temporary("temporary"),
    Device("device"),
    Account("account"),
    Model("model"),
    Repositories("repositories"),
    Ultracode("ultracode"),
    Mcp("mcp"),
    /** #EXP-1793: the MCP servers pick can change here (launch while
     *  temporary; an agent chat, always). */
    McpPick("mcpPick"),
    /** #EXP-1661: the team has at least one skill. */
    Skills("skills"),
    ComputerUse("computerUse"),
    Prompts("prompts"),
    /** #EXP-1715: the message goes to the viewer's Assistant. */
    Assistant("assistant"),
    /** #EXP-1715: the viewer belongs to more than one team. */
    Teams("teams"),
}

data class ComposerMenuConditions(
    /** S-3: launch with the agent row on Temporary session. */
    val temporary: Boolean = false,
    val device: Boolean = false,
    val account: Boolean = false,
    val model: Boolean = false,
    /** The team has more than one repository. */
    val repositories: Boolean = false,
    val ultracode: Boolean = false,
    val mcp: Boolean = false,
    /** #EXP-1793: the MCP servers pick can change here (launch while
     *  temporary; an agent chat, always). */
    val mcpPick: Boolean = false,
    /** #EXP-1661: the team has at least one skill. */
    val skills: Boolean = false,
    val computerUse: Boolean = false,
    /** #EXP-1434: the team has at least one prompt. */
    val prompts: Boolean = false,
    /** #EXP-1715: the message goes to the viewer's Assistant. */
    val assistant: Boolean = false,
    /** #EXP-1715: the viewer belongs to more than one team. */
    val teams: Boolean = false,
) {
    fun holds(condition: ComposerMenuCondition): Boolean = when (condition) {
        ComposerMenuCondition.Temporary -> temporary
        ComposerMenuCondition.Device -> device
        ComposerMenuCondition.Account -> account
        ComposerMenuCondition.Model -> model
        ComposerMenuCondition.Repositories -> repositories
        ComposerMenuCondition.Ultracode -> ultracode
        ComposerMenuCondition.Mcp -> mcp
        ComposerMenuCondition.McpPick -> mcpPick
        ComposerMenuCondition.Skills -> skills
        ComposerMenuCondition.ComputerUse -> computerUse
        ComposerMenuCondition.Prompts -> prompts
        ComposerMenuCondition.Assistant -> assistant
        ComposerMenuCondition.Teams -> teams
    }
}

/** EXP-1635 (composer-menu.json `addFile.phoneChoices`): add-file on a phone. */
enum class ComposerAddFileChoice(val wire: String, val label: String, val icon: String) {
    Camera("camera", "Camera", "ui-camera"),
    Photos("photos", "Photos", "ui-photos"),
    File("file", "File", "ui-file"),
    ;

    companion object {
        /** The sub-choice's rows; Camera only on a device that has one. */
        fun visible(hasCamera: Boolean): List<ComposerAddFileChoice> =
            entries.filter { it != Camera || hasCamera }
    }
}

sealed interface ComposerMenuEntry {
    data class Row(
        val id: ComposerMenuRowId,
        val kind: ComposerMenuRowKind,
        val label: String,
        /** The codex wording of the row, when it has one (`Reasoning`). */
        val codexLabel: String? = null,
        /** A concept id (`packages/icons/icons.json`). */
        val icon: String,
        /** S-3: the composers the row exists on. */
        val surfaces: Set<ComposerMenuSurface> = ComposerMenuSurface.entries.toSet(),
        /** S-3: every condition must hold (`when`: one or a list). */
        val conditions: List<ComposerMenuCondition> = emptyList(),
    ) : ComposerMenuEntry

    data object Separator : ComposerMenuEntry
}

object ComposerMenu {
    const val PLUS_TEST_ID = "agent-composer-plus-button"
    const val MENU_TEST_ID = "agent-composer-menu"
    const val IMPLEMENT_SUBMIT_TEST_ID = "agent-composer-implement"
    const val SUBMENU_SEARCH_TEST_ID = "agent-composer-submenu-search"
    const val PLUS_LABEL = "Add"

    /**
     * SLOP-28 (EXP-1259 D5): Implement issue is the shared search list
     * (`search-list.json`) — one row height, at most this many
     * rows visible before the list scrolls. Phones push them as pages of
     * 44dp rows ([SearchList.ROW_HEIGHT_PHONE]).
     */
    const val SUBMENU_ROW_HEIGHT = 32
    const val SUBMENU_MAX_ROWS = 8

    /** The per-run payload key and the device cap that reads it. */
    const val COMPUTER_USE_PAYLOAD_KEY = "computerUse"

    /** The toggle's shown value: the person's flip, else the device default. */
    fun computerUseShown(pick: Boolean?, deviceDefault: Boolean): Boolean = pick ?: deviceDefault

    /**
     * Web M12 ×4 (iOS/desktop `computer_use_pick`): `computerUse` rides the
     * start only after an explicit flip, i.e. the person's [pick] verbatim;
     * an untouched toggle sends nothing, so the device's own
     * `launch_defaults.computerUse` applies. Null too on a device that does
     * not read the flag ([canToggle] false).
     */
    fun computerUseWire(pick: Boolean?, canToggle: Boolean): Boolean? =
        if (canToggle) pick else null

    val COMPUTER_USE_RUN_CAP: String = DomainContract.codingSessionComputerUseCap

    private val LAUNCH = setOf(ComposerMenuSurface.Launch)
    private val LAUNCH_AND_AGENT = setOf(ComposerMenuSurface.Launch, ComposerMenuSurface.Agent)
    private val ALL = ComposerMenuSurface.entries.toSet()

    /** Every row in fixture order (`rows`). */
    val ROWS: List<ComposerMenuEntry> = listOf(
        ComposerMenuEntry.Row(ComposerMenuRowId.AddFile, ComposerMenuRowKind.Item, "Photo or file", icon = "ui-attach", surfaces = ALL),
        ComposerMenuEntry.Row(ComposerMenuRowId.ImplementIssue, ComposerMenuRowKind.Submenu, "Issue", icon = "editor-issue-ref", surfaces = ALL),
        ComposerMenuEntry.Row(
            ComposerMenuRowId.Prompts, ComposerMenuRowKind.Submenu, "Prompts",
            icon = "coding-command", surfaces = ALL, conditions = listOf(ComposerMenuCondition.Prompts),
        ),
        ComposerMenuEntry.Row(
            ComposerMenuRowId.Teams, ComposerMenuRowKind.Submenu, "Teams",
            icon = "ui-team", surfaces = LAUNCH_AND_AGENT,
            conditions = listOf(ComposerMenuCondition.Assistant, ComposerMenuCondition.Teams),
        ),
        ComposerMenuEntry.Separator,
        ComposerMenuEntry.Row(
            ComposerMenuRowId.Device, ComposerMenuRowKind.Submenu, "Device",
            icon = "nav-devices", surfaces = ALL, conditions = listOf(ComposerMenuCondition.Device),
        ),
        ComposerMenuEntry.Row(
            ComposerMenuRowId.Account, ComposerMenuRowKind.Submenu, "Account",
            icon = "settings-account", surfaces = ALL, conditions = listOf(ComposerMenuCondition.Account),
        ),
        ComposerMenuEntry.Row(
            ComposerMenuRowId.Model, ComposerMenuRowKind.Submenu, "Model",
            icon = "coding-assistant", surfaces = ALL, conditions = listOf(ComposerMenuCondition.Model),
        ),
        ComposerMenuEntry.Row(
            ComposerMenuRowId.Repository, ComposerMenuRowKind.Submenu, "Repository",
            icon = "settings-repositories", surfaces = LAUNCH,
            conditions = listOf(ComposerMenuCondition.Temporary, ComposerMenuCondition.Repositories),
        ),
        ComposerMenuEntry.Separator,
        ComposerMenuEntry.Row(
            ComposerMenuRowId.Effort, ComposerMenuRowKind.Submenu, "Effort", codexLabel = "Reasoning",
            icon = "ui-estimate", surfaces = LAUNCH, conditions = listOf(ComposerMenuCondition.Temporary),
        ),
        ComposerMenuEntry.Row(
            ComposerMenuRowId.Ultracode, ComposerMenuRowKind.Toggle, "Ultracode",
            icon = "action-default", surfaces = LAUNCH,
            conditions = listOf(ComposerMenuCondition.Temporary, ComposerMenuCondition.Ultracode),
        ),
        ComposerMenuEntry.Separator,
        ComposerMenuEntry.Row(
            ComposerMenuRowId.McpServers, ComposerMenuRowKind.Submenu, "MCP servers",
            icon = "ui-mcp", surfaces = LAUNCH_AND_AGENT,
            conditions = listOf(ComposerMenuCondition.McpPick, ComposerMenuCondition.Mcp),
        ),
        ComposerMenuEntry.Row(
            ComposerMenuRowId.Skills, ComposerMenuRowKind.Submenu, "Skills",
            icon = "ui-docs", surfaces = LAUNCH,
            conditions = listOf(ComposerMenuCondition.Temporary, ComposerMenuCondition.Skills),
        ),
        ComposerMenuEntry.Row(
            ComposerMenuRowId.ComputerUse, ComposerMenuRowKind.Toggle, "Computer use",
            icon = "nav-computer", surfaces = LAUNCH,
            conditions = listOf(ComposerMenuCondition.Temporary, ComposerMenuCondition.ComputerUse),
        ),
    )

    fun rowTestId(id: ComposerMenuRowId): String = "agent-composer-menu-${id.wire}"

    /** The rows the "+" shows on [surface] under [conditions] (a row's
     *  `when` = all must hold), separators tidied (never leading, trailing
     *  or doubled). */
    fun composerMenuLayout(surface: ComposerMenuSurface, conditions: ComposerMenuConditions): List<ComposerMenuEntry> {
        val out = mutableListOf<ComposerMenuEntry>()
        for (entry in ROWS) {
            when (entry) {
                ComposerMenuEntry.Separator -> {
                    val last = out.lastOrNull()
                    if (last != null && last != ComposerMenuEntry.Separator) out += ComposerMenuEntry.Separator
                }
                is ComposerMenuEntry.Row -> {
                    if (surface !in entry.surfaces) continue
                    if (entry.conditions.any { !conditions.holds(it) }) continue
                    out += entry
                }
            }
        }
        while (out.lastOrNull() == ComposerMenuEntry.Separator) out.removeAt(out.lastIndex)
        return out
    }

    /** The Implement submenu's footer button once something is picked. */
    fun implementButtonLabel(count: Int): String =
        if (count == 1) "Implement 1 issue" else "Implement $count issues"
}

/**
 * #EXP-1715 (composer-menu.json `teamsPick`): the viewer's Assistant's Teams
 * pick, which teams a turn may touch (`sessions.send` `scopeTeamIds`). null =
 * EVERY team; the last checked team cannot be unchecked; every team checked
 * again = null. Web `composer-menu.ts` `teamsPickValue` + `team-pick.tsx`.
 */
object TeamsPick {
    const val ALL = "All teams"
    const val MANY = "{n} teams"

    data class Team(val id: String, val name: String)

    /** The Teams row's value: every team = [ALL], one = its name, else
     *  "{n} teams"; a picked team the viewer left no longer counts. */
    fun value(picked: List<String>?, teams: List<Team>): String {
        val chosen = if (picked == null) teams else teams.filter { it.id in picked }
        if (chosen.isEmpty() || chosen.size == teams.size) return ALL
        if (chosen.size == 1) return chosen[0].name
        return MANY.replace("{n}", chosen.size.toString())
    }

    /** The teams a pick holds (null = every team), in the teams' order. */
    fun pickedIds(picked: List<String>?, teamIds: List<String>): List<String> =
        teamIds.filter { picked == null || it in picked }

    /** A changed set back to the stored pick: every team = null; none = no
     *  change (the caller keeps the last team checked). */
    fun next(next: Collection<String>, teamIds: List<String>): List<String>? {
        val kept = teamIds.filter { it in next }
        if (kept.isEmpty() || kept.size == teamIds.size) return null
        return kept
    }

    /** The chips: the picked teams while the pick is a SUBSET, else none. */
    fun <T> chips(picked: List<String>?, teams: List<T>, id: (T) -> String): List<T> {
        if (picked == null) return emptyList()
        val chosen = teams.filter { id(it) in picked }
        return if (chosen.isEmpty() || chosen.size == teams.size) emptyList() else chosen
    }

    /** A chip's remove: the last one goes = every team again. */
    fun remove(picked: List<String>?, teamIds: List<String>, removed: String): List<String>? {
        val chosen = pickedIds(picked, teamIds)
        if (chosen.size <= 1) return null
        return next(chosen - removed, teamIds)
    }
}
