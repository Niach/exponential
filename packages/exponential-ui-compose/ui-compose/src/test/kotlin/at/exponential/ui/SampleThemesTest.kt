package at.exponential.ui

import at.exponential.ui.theme.ThemeHandle
import org.junit.Test
import java.io.File

/**
 * Round 4 (VAPP-103): every theme a Compose example or sample ships loads
 * with the STRICT loader (the example's `brand` case of
 * fixtures/theme-extends.json, the samples' `sample.theme.json`), so a
 * theme missing the required `$schema` (or any other refusal) fails CI
 * instead of silently painting the default. Plain JVM.
 */
class SampleThemesTest {
    @Test
    fun everyShippedThemeLoads() {
        Fixtures.require()
        val brand = Fixtures.json("theme-extends.json")["cases"]!!.array!!.first { it["theme"]?.get("id")?.string == "brand" }
        ThemeHandle.load(brand["theme"]!!.json)
        val repo = Fixtures.dir.parentFile!!.parentFile!!.parentFile!!
        ThemeHandle.load(File(repo, "samples/exponential-ui/shared/sample.theme.json").readText())
    }
}
