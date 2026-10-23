package at.exponential.ui

import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import at.exponential.ui.json.JsonValue
import java.io.File

/**
 * VAPP-105: the benchmark suite on the JVM (`packages/exponential-ui-bench/
 * scenarios`, see [BenchSuite]): Robolectric's native text at density 2.75,
 * like [ListBenchTest]. Runs only under `EXPONENTIAL_UI_BENCH=1` (the
 * `exponential.ui.bench` system property; `exponential.ui.benchRepeat` =
 * the in-process repeats, default 3). Writes the fragment to
 * `build/bench-suite.json` (`bun run --filter @exponential-at/ui-bench
 * bench:compose-jvm` picks it up) and prints it.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class BenchSuiteTest {
    @Before
    fun setUp() {
        assumeTrue("EXPONENTIAL_UI_BENCH=1 runs the benchmark suite", System.getProperty("exponential.ui.bench") == "1")
        Fixtures.require()
    }

    @Test
    fun everyScenario() {
        val dir = File(System.getProperty("exponential.ui.benchScenarios") ?: "../../exponential-ui-bench/scenarios")
        assumeTrue("bench scenarios not in this checkout ($dir)", dir.isDirectory)
        val repeat = System.getProperty("exponential.ui.benchRepeat")?.toIntOrNull() ?: 3
        val shaper = robolectricShaper(density = 2.75f)
        val suite = BenchSuite(
            read = { id -> JsonValue.parse(File(dir, "$id.json").readText()) },
            shaper = { shaper },
            memoryMb = {
                System.gc()
                val rt = Runtime.getRuntime()
                (rt.totalMemory() - rt.freeMemory()) / 1048576.0
            },
        )
        val results = suite.run(repeat)
        val os = System.getProperty("os.name").lowercase()
        val platform = if (os.contains("mac")) "macos" else if (os.contains("win")) "windows" else "linux"
        val json = benchFragment("compose-jvm", platform, "Robolectric API 34, ${System.getProperty("os.arch")}, ${Runtime.getRuntime().availableProcessors()} cpus", results)
        File("build").mkdirs()
        File("build/bench-suite.json").writeText(json + "\n")
        println("EXPONENTIAL_UI_BENCH_FRAGMENT $json")
    }
}
