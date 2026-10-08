package at.exponential.ui.kitchensink

import android.content.Intent

/**
 * The launch-extra contract of the kitchen sink (what the shots pipeline,
 * the instrumented tests and the device checks start it with):
 *
 * - `--es shot <view>`: hide the chrome (theme row, host echo stays); `bench` renders [bench] nodes;
 *   an `exponential-ui-<component>` id (or `exponential-ui-demo`) renders that `fixtures/specimens.json`
 *   entry instead of the kitchen sink (VAPP-93, the ui.exponential.at shots).
 * - `--es theme exponential|neutral|playful|brand` (default `exponential`; `brand` = the
 *   `theme-extends.json` acceptance theme: `extends: neutral`, a new primary, the button radius).
 * - `--es mode light|dark` (default `dark`).
 * - `--ez rtl true`: the kitchen sink's root `direction` becomes `rtl`.
 * - `--ei width <dp>`: pin the surface width.
 * - `--es overlays painted`: every layer painted inside the surface.
 * - `--es dump <path>`: two seconds after the first layout, write the WHOLE surface (not the viewport) as a PNG.
 * - `--es a11yDump <path>`: two seconds after the first layout, write the accessibility walk (one label per line).
 * - `--ei bench <n>`: render `benchTreeJson(n)` instead of the kitchen sink.
 * - `--ez benchLoop true`: run the timed pass series after the first layout (logcat tag `ExponentialUI`);
 *   a tap on the host line runs it again.
 */
data class LaunchOptions(
    val shot: String? = null,
    val theme: String = "exponential",
    val mode: String = "dark",
    val rtl: Boolean = false,
    val width: Int? = null,
    val painted: Boolean = false,
    val dump: String? = null,
    val a11yDump: String? = null,
    val bench: Int = 0,
    val benchLoop: Boolean = false,
) {
    companion object {
        /** Read the extras of the launching intent (unknown values fall back to the defaults). */
        fun parse(intent: Intent?): LaunchOptions {
            val x = intent?.extras ?: return LaunchOptions()
            val bench = x.getInt("bench", 0).takeIf { it > 0 } ?: x.getString("bench")?.toIntOrNull() ?: 0
            return LaunchOptions(
                shot = x.getString("shot"),
                theme = x.getString("theme") ?: "exponential",
                mode = x.getString("mode") ?: "dark",
                rtl = x.getBoolean("rtl", false) || x.getString("rtl") == "true",
                width = x.getInt("width", 0).takeIf { it > 0 } ?: x.getString("width")?.toIntOrNull(),
                painted = x.getString("overlays") == "painted",
                dump = x.getString("dump"),
                a11yDump = x.getString("a11yDump"),
                bench = if (bench == 0 && x.getString("shot") == "bench") 200 else bench,
                benchLoop = x.getBoolean("benchLoop", false) || x.getString("benchLoop") == "true",
            )
        }
    }
}
