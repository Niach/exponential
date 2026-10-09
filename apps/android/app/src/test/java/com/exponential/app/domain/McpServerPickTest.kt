package com.exponential.app.domain

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

// EXP-792/EXP-1249: web `lib/mcp-servers.test.ts` rules, same names.
class McpServerPickTest {
    private fun server(id: String, status: String, byDefault: Boolean = false) =
        McpServerOption(id = id, name = id, connectionStatus = status, enabledByDefault = byDefault)

    @Test
    fun `mcpServerReady counts connected and not_needed only`() {
        assertTrue(mcpServerReady(server("a", "connected")))
        assertTrue(mcpServerReady(server("a", "not_needed")))
        assertFalse(mcpServerReady(server("a", "expired")))
        assertFalse(mcpServerReady(server("a", "missing")))
    }

    @Test
    fun `mcpNotReadyLabel says connect or reconnect`() {
        assertNull(mcpNotReadyLabel(server("a", "connected")))
        assertEquals("Reconnect first", mcpNotReadyLabel(server("a", "expired")))
        assertEquals("Reconnect first", mcpNotReadyLabel(server("a", "error")))
        assertEquals("Connect first", mcpNotReadyLabel(server("a", "missing")))
    }

    @Test
    fun `preselectMcpServerIds clamps the saved pick, else the enabled-by-default ready rows`() {
        val servers = listOf(
            server("a", "connected", byDefault = true),
            server("b", "missing", byDefault = true),
            server("c", "not_needed"),
        )
        assertEquals(listOf("a"), preselectMcpServerIds(servers, null))
        assertEquals(listOf("c"), preselectMcpServerIds(servers, listOf("c", "b", "gone")))
        assertEquals(emptyList<String>(), preselectMcpServerIds(servers, emptyList()))
    }

    @Test
    fun `a real action owns its MCP list, a builtin does not`() {
        assertTrue(subjectOwnsMcpServers("a-1"))
        assertFalse(subjectOwnsMcpServers(DomainContract.builtinChatId))
        assertFalse(subjectOwnsMcpServers(null))
        assertNull(mcpPickValue(0))
        assertEquals("2", mcpPickValue(2))
    }
}
