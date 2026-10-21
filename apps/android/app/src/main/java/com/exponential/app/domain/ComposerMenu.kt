package com.exponential.app.domain

// EXP-1249: the composer's ONE "+" menu as a pure layout, the x4 mirror of web
// `components/launch-dialog/composer-menu.ts`, locked against
// `packages/domain-contract/fixtures/composer-menu.json` (`ComposerMenuTest`):
// which rows show for the picked agent / device / subject, in what order,
// with which concept glyph and copy. Phones open it as a bottom sheet of the
// same rows, submenus as pushed pages (`AgentComposer.kt` binds the ids).
// EXP-1259: the suggestion rows are gone (`composer-home.json` owns the page
// around the card). NEW-2: the Run action row is gone (prompts live in the
// `/` menu); the `mcp` condition = the team has a server.

enum class ComposerMenuRowId(val wire: String) {
    ImplementIssue("implement-issue"),
    /** NEW-3: Message an agent ›; `agents` stays false until NEW-7 draws it. */
    MessageAgent("message-agent"),
    AddFile("add-file"),
    Effort("effort"),
    Subagents("subagents"),
    Ultracode("ultracode"),
    McpServers("mcp-servers"),
    ComputerUse("computer-use"),
}

enum class ComposerMenuRowKind { Submenu, Item, Toggle }

/** When a row shows (fixture `conditions`). */
enum class ComposerMenuCondition(val wire: String) {
    SubagentModel("subagentModel"),
    Ultracode("ultracode"),
    Mcp("mcp"),
    ComputerUse("computerUse"),
    Agents("agents"),
}

data class ComposerMenuConditions(
    val subagentModel: Boolean,
    val ultracode: Boolean,
    val mcp: Boolean,
    val computerUse: Boolean,
    /** NEW-3: the team has at least one agent (false until NEW-7). */
    val agents: Boolean = false,
) {
    fun holds(condition: ComposerMenuCondition): Boolean = when (condition) {
        ComposerMenuCondition.SubagentModel -> subagentModel
        ComposerMenuCondition.Ultracode -> ultracode
        ComposerMenuCondition.Mcp -> mcp
        ComposerMenuCondition.ComputerUse -> computerUse
        ComposerMenuCondition.Agents -> agents
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
        val condition: ComposerMenuCondition? = null,
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

    /** Every row in fixture order (`rows`). */
    val ROWS: List<ComposerMenuEntry> = listOf(
        ComposerMenuEntry.Row(ComposerMenuRowId.ImplementIssue, ComposerMenuRowKind.Submenu, "Implement issue", icon = "editor-issue-ref"),
        ComposerMenuEntry.Row(
            ComposerMenuRowId.MessageAgent, ComposerMenuRowKind.Submenu, "Message an agent",
            icon = "settings-agents", condition = ComposerMenuCondition.Agents,
        ),
        ComposerMenuEntry.Row(ComposerMenuRowId.AddFile, ComposerMenuRowKind.Item, "Add file or image", icon = "ui-attach"),
        ComposerMenuEntry.Separator,
        ComposerMenuEntry.Row(ComposerMenuRowId.Effort, ComposerMenuRowKind.Submenu, "Effort", codexLabel = "Reasoning", icon = "ui-estimate"),
        ComposerMenuEntry.Row(
            ComposerMenuRowId.Subagents, ComposerMenuRowKind.Submenu, "Subagents",
            icon = "coding-subagent", condition = ComposerMenuCondition.SubagentModel,
        ),
        ComposerMenuEntry.Row(
            ComposerMenuRowId.Ultracode, ComposerMenuRowKind.Toggle, "Ultracode",
            icon = "action-default", condition = ComposerMenuCondition.Ultracode,
        ),
        ComposerMenuEntry.Separator,
        ComposerMenuEntry.Row(
            ComposerMenuRowId.McpServers, ComposerMenuRowKind.Submenu, "MCP servers",
            icon = "ui-mcp", condition = ComposerMenuCondition.Mcp,
        ),
        ComposerMenuEntry.Row(
            ComposerMenuRowId.ComputerUse, ComposerMenuRowKind.Toggle, "Computer use",
            icon = "nav-computer", condition = ComposerMenuCondition.ComputerUse,
        ),
    )

    fun rowTestId(id: ComposerMenuRowId): String = "agent-composer-menu-${id.wire}"

    /** The rows the "+" shows under [conditions], separators tidied (never
     *  leading, trailing or doubled). */
    fun composerMenuLayout(conditions: ComposerMenuConditions): List<ComposerMenuEntry> {
        val out = mutableListOf<ComposerMenuEntry>()
        for (entry in ROWS) {
            when (entry) {
                ComposerMenuEntry.Separator -> {
                    val last = out.lastOrNull()
                    if (last != null && last != ComposerMenuEntry.Separator) out += ComposerMenuEntry.Separator
                }
                is ComposerMenuEntry.Row -> {
                    if (entry.condition != null && !conditions.holds(entry.condition)) continue
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
