package at.exponential.ui

import at.exponential.ui.catalog.CatalogConstants
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import java.io.File

/**
 * `CatalogConstants` reads the generated catalog object compiled in from
 * `packages/exponential-ui/generated/ExponentialUICatalog.generated.kt`:
 * the compiled values equal the file on disk (a stale copy fails here).
 */
class CatalogConstantsTest {
    @Before
    fun setUp() = Fixtures.require()

    private val generated: String by lazy { File(Fixtures.dir.parentFile, "generated/ExponentialUICatalog.generated.kt").readText() }

    private fun list(name: String): List<String> {
        val line = generated.lines().first { it.trimStart().startsWith("val $name: List<String> = listOf(") }
        return Regex("\"((?:[^\"\\\\]|\\\\.)*)\"").findAll(line.substringAfter("listOf(")).map { it.groupValues[1] }.toList()
    }

    @Test
    fun theMirrorMatchesTheGeneratedTable() {
        val names = list("layoutConstantNames")
        val values = list("layoutConstantValues").map { it.toFloat() }
        assertEquals(names.zip(values).toMap(), CatalogConstants.layout)
        assertEquals(list("rtlMirroredIcons"), CatalogConstants.rtlMirroredIcons)
        assertEquals(list("animationNames"), CatalogConstants.animationNames)
    }

    @Test
    fun aDollarInTheGeneratedStringsIsLiteral() {
        // `$breakpoint` in a Kotlin string is a template unless escaped.
        assertTrue(ExponentialUICatalog.styleMediaPattern, ExponentialUICatalog.styleMediaPattern.contains("|\\\$breakpoint\\.[a-zA-Z0-9]+)"))
        assertTrue(Regex(ExponentialUICatalog.styleMediaPattern).matches("@media (min-width: \$breakpoint.md)"))
    }
}
