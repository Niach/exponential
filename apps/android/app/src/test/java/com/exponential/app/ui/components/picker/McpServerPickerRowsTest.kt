package com.exponential.app.ui.components.picker

import androidx.compose.ui.graphics.Color
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.McpServerGlyph
import com.exponential.app.domain.McpServerOption
import com.exponential.app.domain.mcpCatalogMarkFor
import com.exponential.app.domain.mcpHostOf
import com.exponential.app.domain.mcpServerCaption
import com.exponential.app.domain.mcpServerGlyph
import com.exponential.app.ui.icons.ExpBrandIcons
import com.exponential.app.ui.icons.ExpIcons
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

// EXP-1793: web packages/ui/src/picker/mcp-server-picker.tsx rows — the host
// (or command) caption, the not-ready note in its place, the catalog glyph.
class McpServerPickerRowsTest {
    private fun server(
        status: String = "connected",
        url: String? = null,
        command: String? = null,
    ) = McpServerOption(id = "s", name = "S", connectionStatus = status, url = url, command = command)

    @Test
    fun `mcpHostOf reads web's URL host`() {
        assertEquals("mcp.linear.app", mcpHostOf(" https://MCP.Linear.app/mcp "))
        assertEquals("localhost:8080", mcpHostOf("http://localhost:8080/mcp"))
        assertEquals("example.com", mcpHostOf("https://example.com:443/x"))
        assertNull(mcpHostOf("not a url"))
        assertNull(mcpHostOf(null))
        assertNull(mcpHostOf(""))
    }

    @Test
    fun `mcpCatalogMarkFor matches a hosted entry by exact host, else a template by path`() {
        assertEquals("linear", mcpCatalogMarkFor("https://mcp.linear.app/sse"))
        assertEquals("github", mcpCatalogMarkFor("https://api.githubcopilot.com/mcp/"))
        assertEquals("gitlab", mcpCatalogMarkFor("https://gitlab.example.org/api/v4/mcp"))
        assertNull(mcpCatalogMarkFor("https://linear.app/mcp"))
        assertNull(mcpCatalogMarkFor("https://gitlab.example.org/other"))
        assertNull(mcpCatalogMarkFor(null))
    }

    @Test
    fun `mcpServerGlyph is the brand mark, a shell for a command, else the plug`() {
        assertEquals(McpServerGlyph.Brand("notion"), mcpServerGlyph(server(url = "https://mcp.notion.com/mcp")))
        assertEquals(McpServerGlyph.Shell, mcpServerGlyph(server(command = "npx -y srv")))
        assertEquals(McpServerGlyph.Plug, mcpServerGlyph(server(url = "https://mcp.internal.dev/mcp")))
        assertEquals(McpServerGlyph.Plug, mcpServerGlyph(server()))
    }

    @Test
    fun `mcpServerCaption is the host or command, the not-ready note replacing it`() {
        assertEquals("mcp.linear.app", mcpServerCaption(server(url = "https://mcp.linear.app/mcp")))
        assertEquals("npx -y srv", mcpServerCaption(server(command = "npx -y srv")))
        assertEquals("Connect first", mcpServerCaption(server(status = "missing", url = "https://mcp.linear.app/mcp")))
        assertEquals("Reconnect first", mcpServerCaption(server(status = "expired", command = "npx")))
        assertNull(mcpServerCaption(server()))
    }

    @Test
    fun `mcpServerGlyphIcon draws every catalog mark, the plug for an unknown slug`() {
        for (mark in DomainContract.mcpCatalogMarks) {
            assertEquals(ExpBrandIcons.bySlug(mark)!!, mcpServerGlyphIcon(McpServerGlyph.Brand(mark)))
        }
        assertEquals(ExpIcons.uiMcp, mcpServerGlyphIcon(McpServerGlyph.Brand("nope")))
        assertEquals(ExpIcons.uiMcp, mcpServerGlyphIcon(McpServerGlyph.Plug))
        assertEquals(ExpIcons.sessionShell, mcpServerGlyphIcon(McpServerGlyph.Shell))
    }

    @Test
    fun `mcpServerPickerItems carries caption, glyph, disabled and keywords`() {
        val (ready, shell, missing) = mcpServerPickerItems(
            listOf(
                server(url = "https://mcp.linear.app/mcp").copy(id = "a", name = "Linear"),
                server(status = "not_needed", command = "uvx tool").copy(id = "b", name = "Tool"),
                server(status = "missing", url = "https://x.dev/mcp").copy(id = "c", name = "X"),
            ),
        )
        assertEquals("mcp.linear.app", ready.description)
        assertEquals(ExpBrandIcons.linear, ready.icon)
        assertEquals(Color.Unspecified, ready.color)
        assertFalse(ready.disabled)
        assertEquals(listOf("Linear", "mcp.linear.app"), ready.keywords)
        assertEquals("uvx tool", shell.description)
        assertEquals(ExpIcons.sessionShell, shell.icon)
        assertNull(shell.color)
        assertEquals("Connect first", missing.description)
        assertEquals(ExpIcons.uiMcp, missing.icon)
        assertNull(missing.color)
        assertTrue(missing.disabled)
    }
}
