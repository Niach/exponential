package com.exponential.app.ui.components.picker

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import com.exponential.app.domain.McpServerOption
import com.exponential.app.domain.mcpNotReadyLabel
import com.exponential.app.ui.icons.ExpIcons

/**
 * EXP-792/EXP-1249: the MCP servers multiselect (web `McpServerPicker`): the
 * team's servers by the `ui-mcp` glyph + name, multi (the row's highlight is
 * the pick). A server the caller has not connected (or whose sign-in expired)
 * is DISABLED with `Connect first` / `Reconnect first` under its name: a
 * phone has no MCP settings, so connecting happens on web or the IDE.
 */
fun mcpServerPickerItems(servers: List<McpServerOption>): List<PickerItem<String>> =
    servers.map { server ->
        val note = mcpNotReadyLabel(server)
        PickerItem(
            value = server.id,
            label = server.name,
            icon = ExpIcons.uiMcp,
            description = note,
            disabled = note != null,
        )
    }

@Composable
fun McpServerPicker(
    servers: List<McpServerOption>,
    value: List<String>,
    /** One toggled server id (added or removed). */
    onToggle: (String) -> Unit,
    title: String = "MCP servers",
    sheetModifier: Modifier = Modifier,
    open: Boolean? = null,
    onOpenChange: ((Boolean) -> Unit)? = null,
    trigger: @Composable (open: () -> Unit) -> Unit = {},
) {
    Picker(
        items = mcpServerPickerItems(servers),
        mode = PickerMode.Multi,
        value = value.toSet(),
        onChange = { next ->
            val added = next.firstOrNull { it !in value }
            val removed = value.firstOrNull { it !in next }
            (added ?: removed)?.let(onToggle)
        },
        search = servers.size > 6,
        emptyText = "No MCP servers",
        title = title,
        sheetModifier = sheetModifier,
        open = open,
        onOpenChange = onOpenChange,
        trigger = trigger,
    )
}
