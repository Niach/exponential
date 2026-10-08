package at.exponential.ui

import at.exponential.ui.catalog.CatalogConstants
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test
import java.io.File

/**
 * `CatalogConstants` mirrors the generated catalog table
 * (`packages/exponential-ui/generated/ExponentialUICatalog.generated.kt`):
 * a catalog change fails here until the mirror is copied.
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
}
