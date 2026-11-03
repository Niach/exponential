package com.exponential.app.domain

import at.exponential.ui.json.JsonValue
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Test

// VAPP-234: locks the gallery and its copy to
// `packages/domain-contract/fixtures/app-templates.json`.
class AppTemplatesTest {
    private val fixture: JsonValue = run {
        val relative = "packages/domain-contract/fixtures/app-templates.json"
        val file = listOf("../../../$relative", "../../$relative", relative).map(::File).firstOrNull { it.isFile }
            ?: error("$relative not found from ${File(".").absolutePath}")
        JsonValue.parse(file.readText())
    }

    @Test
    fun `the list, the copy and the test ids match the fixture`() {
        val templates = fixture["templates"]!!.array!!.map {
            AppTemplates.Template(it["id"]!!.string!!, it["name"]!!.string!!, it["icon"]!!.string!!, it["color"]!!.string!!, it["description"]!!.string!!)
        }
        assertEquals(templates, AppTemplates.TEMPLATES)
        val copy = fixture["copy"]!!
        assertEquals(copy["title"]!!.string, AppTemplates.Copy.TITLE)
        assertEquals(copy["fromTemplate"]!!.string, AppTemplates.Copy.FROM_TEMPLATE)
        assertEquals(copy["includeRecords"]!!.string, AppTemplates.Copy.INCLUDE_RECORDS)
        assertEquals(copy["note"]!!.string, AppTemplates.Copy.NOTE)
        val ids = fixture["testIds"]!!
        assertEquals(ids["row"]!!.string, AppTemplates.TestIds.ROW)
        assertEquals(ids["records"]!!.string, AppTemplates.TestIds.RECORDS)
        assertEquals(ids["fromTemplate"]!!.string, AppTemplates.TestIds.FROM_TEMPLATE)
    }
}
