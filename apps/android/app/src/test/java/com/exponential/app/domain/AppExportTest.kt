package com.exponential.app.domain

import at.exponential.ui.json.JsonValue
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Test

// VAPP-121: replays `packages/domain-contract/fixtures/app-export.json`'s
// `exportCases` (web, desktop and iOS replay the same cases; compared as
// parsed JSON, key order free) and locks the format, the copy and the limits.
class AppExportTest {
    private fun read(relative: String): JsonValue {
        val candidates = listOf("../../../$relative", "../../$relative", relative)
        val file = candidates.map(::File).firstOrNull { it.isFile } ?: error("$relative not found from ${File(".").absolutePath}")
        return JsonValue.parse(file.readText())
    }

    private val fixture = read("packages/domain-contract/fixtures/app-export.json")

    private fun nullable(v: JsonValue?): JsonValue? = v?.takeIf { it != JsonValue.Null }

    @Test
    fun `the format, the copy and the limits match the fixture`() {
        assertEquals(fixture["format"]!!.string, AppExport.FORMAT)
        assertEquals(fixture["fileSuffix"]!!.string, AppExport.SUFFIX)
        assertEquals(fixture["mime"]!!.string, AppExport.MIME)
        val limits = fixture["limits"]!!
        assertEquals(limits["importMaxBytes"]!!.number!!.toInt(), AppExport.Limits.IMPORT_MAX_BYTES)
        assertEquals(limits["recordsMax"]!!.number!!.toInt(), AppExport.Limits.RECORDS_MAX)
        assertEquals(limits["issuesShown"]!!.number!!.toInt(), AppExport.Limits.ISSUES_SHOWN)
        val copy = fixture["copy"]!!
        val ours = mapOf(
            "export" to AppExport.Copy.EXPORT,
            "exportTitle" to AppExport.Copy.EXPORT_TITLE,
            "includeRecords" to AppExport.Copy.INCLUDE_RECORDS,
            "import" to AppExport.Copy.IMPORT,
            "importTitle" to AppExport.Copy.IMPORT_TITLE,
            "importPlaceholder" to AppExport.Copy.IMPORT_PLACEHOLDER,
            "chooseFile" to AppExport.Copy.CHOOSE_FILE,
            "importAction" to AppExport.Copy.IMPORT_ACTION,
            "importNote" to AppExport.Copy.IMPORT_NOTE,
            "moreIssues" to AppExport.Copy.MORE_ISSUES,
        )
        assertEquals(copy.obj!!.keys, ours.keys)
        for ((key, value) in ours) assertEquals(key, copy[key]!!.string, value)
        assertEquals("Export Working hours", AppExport.exportTitle("Working hours"))
        assertEquals("and 3 more", AppExport.moreIssues(3))
        val issue = fixture["capture"]!!["issues"]!!.array!!.first()
        assertEquals("/package: is required", AppExport.issueLine(AppExport.Issue(issue["path"]!!.string!!, issue["message"]!!.string!!)))
    }

    @Test
    fun exportCases() {
        val cases = fixture["exportCases"]!!.array!!
        assert(cases.isNotEmpty())
        for (case in cases) {
            val name = case["name"]!!.string!!
            val input = case["input"]!!
            val app = input["app"]!!
            val records = input["records"]?.array.orEmpty().map {
                AppExport.Record(it["id"]!!.string!!, it["kind"]!!.string!!, it["data"]!!, it["createdAt"]!!.string!!)
            }
            val file = AppExport.buildAppExport(
                app = AppExport.App(
                    name = app["name"]!!.string!!,
                    icon = nullable(app["icon"])?.string,
                    color = nullable(app["color"])?.string,
                    description = nullable(app["description"])?.string,
                ),
                pkg = input["package"]!!,
                records = records,
                includeRecords = input["includeRecords"]!!.bool!!,
            )
            val expected = case["expected"]!!
            assertEquals(name, expected["fileName"]!!.string, AppExport.appExportFileName(app["slug"]!!.string!!))
            assertEquals(name, expected["file"], file)
            // The written text parses back to the same file.
            val text = AppExport.appExportText(file)
            assert(text.endsWith("}\n")) { name }
            assertEquals(name, expected["file"], JsonValue.parse(text))
        }
    }
}
