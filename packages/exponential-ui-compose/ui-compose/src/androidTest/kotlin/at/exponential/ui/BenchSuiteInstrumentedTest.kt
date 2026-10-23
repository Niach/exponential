package at.exponential.ui

import android.os.Build
import android.os.Debug
import android.util.Log
import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.font.createFontFamilyResolver
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import at.exponential.ui.json.JsonValue
import at.exponential.ui.measure.TextShaper
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith

/**
 * VAPP-105: the benchmark suite on a device or emulator (CI:
 * `exponential-ui.yml` `compose-emulator`): the scenarios come from the
 * test APK's assets (`bench/<id>.json`, copied from
 * `packages/exponential-ui-bench/scenarios` by the `exponentialUiBenchScenarios`
 * Gradle task), the text measure is the device's. Runs ONLY when the
 * instrumentation argument `bench` is `1` (`runners/compose-android.ts`
 * passes it; the plain `connectedDebugAndroidTest` skips it), 3 repeats;
 * the fragment leaves the device as `Log.i("ExponentialUIBench", …)` lines
 * (`EXPONENTIAL_UI_BENCH_FRAGMENT <i>/<n> <chunk>`, 4000-char chunks) that
 * `runners/compose-android.ts` reads back with `adb logcat`.
 */
@RunWith(AndroidJUnit4::class)
class BenchSuiteInstrumentedTest {
    @Test
    fun everyScenario() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        assumeTrue("bench=1 not passed", InstrumentationRegistry.getArguments().getString("bench") == "1")
        val context = instrumentation.targetContext
        ExponentialUi.appContext = context.applicationContext
        val density = context.resources.displayMetrics.density
        val shaper = TextShaper(TextMeasurer(createFontFamilyResolver(context), Density(density, 1f), LayoutDirection.Ltr), density) { null }
        val assets = instrumentation.context.assets
        val suite = BenchSuite(
            read = { id -> JsonValue.parse(assets.open("bench/$id.json").bufferedReader().use { it.readText() }) },
            shaper = { shaper },
            memoryMb = {
                System.gc()
                val rt = Runtime.getRuntime()
                ((rt.totalMemory() - rt.freeMemory()) + Debug.getNativeHeapAllocatedSize()) / 1048576.0
            },
        )
        // The model is main-thread only (its snapshot state, the text
        // measurer): the suite runs where the app would run it.
        var results: Map<String, Map<String, Double>> = emptyMap()
        var failure: Throwable? = null
        instrumentation.runOnMainSync {
            try {
                results = suite.run(3)
            } catch (t: Throwable) {
                failure = t
            }
        }
        failure?.let { throw it }
        val emulator = Build.FINGERPRINT.contains("generic") || Build.HARDWARE.contains("ranchu") || Build.HARDWARE.contains("goldfish")
        val device = "${Build.MANUFACTURER} ${Build.MODEL} (API ${Build.VERSION.SDK_INT})${if (emulator) " emulator" else ""}"
        val json = benchFragment("compose-android", "android", device, results)
        val chunks = json.chunked(4000)
        chunks.forEachIndexed { i, c -> Log.i(TAG, "EXPONENTIAL_UI_BENCH_FRAGMENT ${i + 1}/${chunks.size} $c") }
    }

    companion object {
        const val TAG = "ExponentialUIBench"
    }
}
