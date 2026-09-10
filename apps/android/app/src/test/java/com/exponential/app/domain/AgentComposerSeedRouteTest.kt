package com.exponential.app.domain

import java.net.URLDecoder
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-825: the composer's preselection rides the `agent?…` route's query args.
 * The mint ([agentRoute]) and the read ([AgentComposerSeed.fromArgs]) must be
 * inverses through Navigation's percent-decoding, and the web rules hold:
 * `action` wins over `issues`, empty params are omitted, bad ids are dropped.
 */
class AgentComposerSeedRouteTest {

    private val issueA = "3f9d2a1c-1111-4aaa-8bbb-000000000001"
    private val issueB = "3f9d2a1c-2222-4aaa-8bbb-000000000002"

    /** What Navigation hands the ViewModel back: the decoded query values. */
    private fun argsOf(route: String): (String) -> String? {
        val query = route.substringAfter('?', "")
        val map = query.split('&').filter { it.isNotEmpty() }.associate { pair ->
            val key = pair.substringBefore('=')
            val value = URLDecoder.decode(pair.substringAfter('='), "UTF-8")
            key to value
        }
        return { name -> map[name] }
    }

    @Test
    fun `an empty seed is the bare route`() {
        assertEquals("agent", agentRoute(AgentComposerSeed.EMPTY))
        assertEquals("agent", agentRoute(AgentComposerSeed(text = "", icon = "", actionId = "")))
        val parsed = AgentComposerSeed.fromArgs { null }
        assertEquals(AgentComposerSeed.EMPTY, parsed)
        assertFalse(parsed.hasSubject)
    }

    @Test
    fun `the pattern declares every arg as a query placeholder`() {
        assertEquals(
            "agent?issues={issues}&action={action}&device={device}&pr={pr}&text={text}&icon={icon}",
            AGENT_ROUTE_PATTERN,
        )
        assertEquals(listOf("issues", "action", "device", "pr", "text", "icon"), AGENT_ROUTE_ARGS)
    }

    @Test
    fun `issues ride as a csv and only picked params appear`() {
        val route = agentRoute(AgentComposerSeed(issueIds = listOf(issueA, issueB)))
        assertEquals("agent?issues=$issueA%2C$issueB", route)
        val parsed = AgentComposerSeed.fromArgs(argsOf(route))
        assertEquals(listOf(issueA, issueB), parsed.issueIds)
        assertNull(parsed.actionId)
        assertNull(parsed.deviceId)
        assertNull(parsed.text)
        assertTrue(parsed.hasSubject)
    }

    @Test
    fun `a full seed round-trips through percent-encoding`() {
        val text = "Triage the nightly failures\n\nAutomation — call `x` with {\"a\":1}&b=2 +100%"
        val seed = AgentComposerSeed(
            issueIds = listOf(issueA),
            actionId = "builtin:create-action",
            deviceId = "dev-1",
            prIssueId = issueB,
            text = text,
            icon = "sparkles",
        )
        val route = agentRoute(seed)
        // Nothing unencoded that could break the query: no raw `&`, `=`, `+`,
        // whitespace or newline inside a value.
        val values = route.substringAfter('?').split('&').map { it.substringAfter('=') }
        values.forEach { value ->
            assertFalse(value, value.any { it == '&' || it == '=' || it == '+' || it.isWhitespace() })
        }
        assertEquals(seed, AgentComposerSeed.fromArgs(argsOf(route)))
    }

    @Test
    fun `action wins over issues for the checked set`() {
        val seed = AgentComposerSeed(issueIds = listOf(issueA), actionId = "builtin:fix-conflicts")
        assertTrue(seed.effectiveIssueIds.isEmpty())
        assertTrue(seed.hasSubject)
        assertEquals(listOf(issueA), AgentComposerSeed(issueIds = listOf(issueA)).effectiveIssueIds)
    }

    @Test
    fun `malformed ids are dropped rather than refused`() {
        val parsed = AgentComposerSeed.fromArgs(
            argsOf("agent?issues=$issueA%2Cnot-a-uuid%2C%20&pr=nope&action=&device=&text=&icon="),
        )
        assertEquals(listOf(issueA), parsed.issueIds)
        assertNull(parsed.prIssueId)
        assertNull(parsed.actionId)
        assertNull(parsed.deviceId)
        assertNull(parsed.text)
        assertNull(parsed.icon)
    }

    @Test
    fun `route values keep unreserved characters verbatim`() {
        assertEquals("abc-DEF_1.2~", encodeRouteValue("abc-DEF_1.2~"))
        assertEquals("a%20b%2Fc%3Fd%23e%C3%A9", encodeRouteValue("a b/c?d#eé"))
    }
}
