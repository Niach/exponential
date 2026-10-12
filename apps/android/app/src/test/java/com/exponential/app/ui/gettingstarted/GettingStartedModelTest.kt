package com.exponential.app.ui.gettingstarted

import com.exponential.app.domain.contractFixtureJson
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

// EXP-1270 (EXP-1259 D6): `deriveGettingStarted`, replayed off
// `getting-started.json` — every case, both platforms, the WHOLE expected
// object (web `getting-started-model.test.ts`, same case names).
class GettingStartedModelTest {
    private val fixture: JsonObject =
        Json.parseToJsonElement(contractFixtureJson("getting-started.json")).jsonObject

    private fun JsonObject.bool(key: String): Boolean = getValue(key).jsonPrimitive.boolean
    private fun JsonObject.string(key: String): String = getValue(key).jsonPrimitive.content

    @Test
    fun `deriveGettingStarted replays every case`() {
        val cases = fixture.getValue("cases").jsonArray
        assertTrue(cases.isNotEmpty())
        val platforms = mutableSetOf<GettingStartedPlatform>()
        for (element in cases) {
            val case = element.jsonObject
            val s = case.getValue("signals").jsonObject
            val p = case.getValue("permissions").jsonObject
            val platform = GettingStartedPlatform.entries.first { it.wire == case.string("platform") }
            platforms += platform
            val got = GettingStarted.derive(
                GettingStartedSignals(
                    hasBoard = s.bool("hasBoard"),
                    githubInstalled = s.bool("githubInstalled"),
                    hasDesktopDevice = s.bool("hasDesktopDevice"),
                    hasServerDevice = s.bool("hasServerDevice"),
                    hasCodingSession = s.bool("hasCodingSession"),
                    hasInvitedTeam = s.bool("hasInvitedTeam"),
                    mcpConnected = s.bool("mcpConnected"),
                    hasWidget = s.bool("hasWidget"),
                ),
                GettingStartedPermissions(
                    canManageMembers = p.bool("canManageMembers"),
                    canManageWidgets = p.bool("canManageWidgets"),
                ),
                platform,
            )
            val e = case.getValue("expected").jsonObject
            val want = GettingStartedModel(
                entries = e.getValue("entries").jsonArray.map { it.jsonObject }.map { entry ->
                    GettingStartedEntry(
                        key = GettingStartedStepKey.entries.first { it.wire == entry.string("key") },
                        done = entry.bool("done"),
                        target = GettingStartedTarget.entries.first { it.wire == entry.string("target") },
                        next = entry.bool("next"),
                    )
                },
                done = e.getValue("done").jsonPrimitive.int,
                total = e.getValue("total").jsonPrimitive.int,
                progress = e.string("progress"),
                complete = e.bool("complete"),
            )
            assertEquals(case.string("name"), want, got)
        }
        // The model takes every platform; the fixture exercises both.
        assertEquals(GettingStartedPlatform.entries.toSet(), platforms)
    }

    @Test
    fun `the steps are the fixture's`() {
        val steps = fixture.getValue("steps").jsonArray.map { it.jsonObject }
        assertEquals(steps.size, GettingStarted.STEPS.size)
        steps.zip(GettingStarted.STEPS).forEach { (want, got) ->
            assertEquals(want.string("key"), got.key.wire)
            assertEquals(want.string("title"), got.title)
            assertEquals(want.string("icon"), got.icon)
            assertEquals(want.string("target"), got.target.wire)
            assertEquals(want["needsDevice"]?.jsonPrimitive?.boolean ?: false, got.needsDevice)
            assertEquals(want.bool("phones"), got.phones)
            assertEquals(want["visibleWhen"]?.jsonPrimitive?.content, got.visibleWhen)
        }
    }

    @Test
    fun `the block hides while loading and once complete`() {
        assertEquals(false, GettingStartedState().visible)
        val fresh = GettingStarted.derive(
            GettingStartedSignals(),
            GettingStartedPermissions(canManageMembers = true),
            GettingStartedPlatform.Phone,
        )
        assertEquals(true, GettingStartedState(fresh, loading = false).visible)
        val all = GettingStarted.derive(
            GettingStartedSignals(
                hasBoard = true, githubInstalled = true, hasDesktopDevice = true, hasServerDevice = true,
                hasCodingSession = true, hasInvitedTeam = true,
            ),
            GettingStartedPermissions(canManageMembers = true),
            GettingStartedPlatform.Phone,
        )
        assertEquals(false, GettingStartedState(all, loading = false).visible)
        assertEquals(true, GettingStartedState(all, loading = false).complete)
    }
}
