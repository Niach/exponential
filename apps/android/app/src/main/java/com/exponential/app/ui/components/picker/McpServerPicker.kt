package com.exponential.app.ui.components.picker

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import com.exponential.app.domain.McpServerGlyph
import com.exponential.app.domain.McpServerOption
import com.exponential.app.domain.mcpServerCaption
import com.exponential.app.domain.mcpServerGlyph
import com.exponential.app.domain.mcpServerTarget
import com.exponential.app.domain.mcpNotReadyLabel
import com.exponential.app.ui.icons.ExpBrandIcons
import com.exponential.app.ui.icons.ExpIcons

/**
 * EXP-792/EXP-1249: the MCP servers multiselect (web `McpServerPicker`): the
 * team's servers by glyph + name, multi (the row's highlight is the pick).
 * EXP-1793: the muted second line is the server's host (or its command), like
 * web and the IDE; a server the caller has not connected (or whose sign-in
 * expired) is DISABLED with `Connect first` / `Reconnect first` in its place:
 * a phone has no MCP settings, so connecting happens on web or the IDE.
 * The glyph: a catalog server's REAL brand mark ([ExpBrandIcons], drawn
 * UNTINTED so its own white fills read like web's), a terminal for a stdio
 * command, else the plug.
 */
fun mcpServerPickerItems(servers: List<McpServerOption>): List<PickerItem<String>> =
    servers.map { server ->
        val note = mcpNotReadyLabel(server)
        val target = mcpServerTarget(server)
        val glyph = mcpServerGlyph(server)
        val icon = mcpServerGlyphIcon(glyph)
        PickerItem(
            value = server.id,
            label = server.name,
            icon = icon,
            // A drawn brand mark keeps its own fills (the row's tint is `color`).
            color = if (icon !== ExpIcons.uiMcp && glyph is McpServerGlyph.Brand) Color.Unspecified else null,
            description = mcpServerCaption(server),
            disabled = note != null,
            keywords = listOfNotNull(server.name, target),
        )
    }

/** [McpServerGlyph] → the drawn vector; an unknown brand slug falls back to the plug. */
fun mcpServerGlyphIcon(glyph: McpServerGlyph): ImageVector = when (glyph) {
    is McpServerGlyph.Brand -> ExpBrandIcons.bySlug(glyph.mark) ?: ExpIcons.uiMcp
    McpServerGlyph.Shell -> ExpIcons.sessionShell
    McpServerGlyph.Plug -> ExpIcons.uiMcp
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
        checkMarks = true,
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
