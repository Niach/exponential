package at.exponential.runner

import at.exponential.engine.ffi.NetworkPolicy
import at.exponential.ui.json.JsonValue
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/** runner.json `bundle` + `configCases` through the runner's reading of `assets/app` (the real Engine over JNA). */
class RunnerBundleTest {
    @Test
    fun configCasesReplay() {
        val fixture = Fixtures.runner()
        for (case in fixture["configCases"]!!.array!!) {
            val got = RunnerBundle.config(case["input"]!!.json)
            val expected = case["expected"]!!["config"]!!
            if (expected.isNull) {
                assertNull(case["name"]!!.string, got)
            } else {
                assertEquals(case["name"]!!.string, expected["id"]!!.string, got!!.id)
                assertEquals(expected["name"]!!.string, got.name)
                assertEquals(expected["version"]!!.string, got.version)
                val network = mapOf("off" to NetworkPolicy.OFF, "upstreamsOnly" to NetworkPolicy.UPSTREAMS_ONLY, "any" to NetworkPolicy.ANY)
                assertEquals(network[expected["network"]!!.string], got.network)
            }
        }
        assertNull("not JSON", RunnerBundle.config("{"))
    }

    @Test
    fun theBundleLayoutIsTheContracts() {
        val bundle = Fixtures.runner()["bundle"]!!
        assertEquals(bundle["dir"]!!.string, RunnerBundle.DIR)
        assertEquals(bundle["files"]!!["config"]!!.string, RunnerBundle.CONFIG)
        assertEquals(bundle["files"]!!["package"]!!.string, RunnerBundle.PACKAGE)
        assertEquals("assets/app (the APK's assets)", bundle["locations"]!!["android"]!!.string)
        assertEquals(Fixtures.runner()["storage"]!!["deviceKey"]!!.string, RunnerController.DEVICE_KEY)
        assertEquals(Fixtures.lifecycle()["copy"]!!["refused"]!!.string, RunnerController.LINK_REFUSED)
    }

    @Test
    fun runnerJsonFirstThenThePackage() {
        val good = """{"id":"at.acme.album","name":"Album","version":"1.0.0"}"""
        assertEquals(BundleLoad.Failed("config"), RunnerBundle.read { null })
        assertEquals(BundleLoad.Failed("config"), RunnerBundle.read { if (it == RunnerBundle.CONFIG) """{"id":"album"}""" else "{}" })
        assertEquals(BundleLoad.Failed("config"), RunnerBundle.read { if (it == RunnerBundle.CONFIG) """{"id":"at.a.b","name":"A","version":"1.0.0","telemetry":"x"}""" else "{}" })
        assertEquals(BundleLoad.Failed("missing"), RunnerBundle.read { if (it == RunnerBundle.CONFIG) good else null })
        val ready = RunnerBundle.read { if (it == RunnerBundle.CONFIG) good else "{}" } as BundleLoad.Ready
        assertEquals(NetworkPolicy.OFF, ready.config.network)
    }

    /** The template's own placeholder bundle reads, and its package loads in the Engine. */
    @Test
    fun theTemplatesBundleRuns() {
        val dir = File(System.getProperty("runner.assets"), RunnerBundle.DIR)
        val load = RunnerBundle.read { name -> File(dir, name).takeIf { it.exists() }?.readText() }
        assertTrue(load is BundleLoad.Ready)
        val harness = Harness()
        assertTrue(harness.controller.start(load) { harness.engine(it) })
        val pkg = JsonValue.parse(harness.controller.app!!.packageJson())
        assertEquals(listOf("notes", "photos"), pkg["screens"]!!.array!!.map { it["template"]!!.string })
        assertTrue("an ask scope = the Permissions entry", pkg["capabilities"]!!.array!!.isNotEmpty())
    }
}
