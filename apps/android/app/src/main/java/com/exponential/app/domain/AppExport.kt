package com.exponential.app.domain

import at.exponential.ui.json.JsonValue

// VAPP-121: an app as ONE JSON file, the Android mirror of the pure reference
// `packages/domain-contract/src/app-export.ts` (fixtures/app-export.json is
// the spec; `AppExportTest` replays its `exportCases` and locks [Copy],
// [Limits], [FORMAT], [SUFFIX] and [MIME] to the fixture). The import half
// (`parseAppImport`) runs on the SERVER (`apps.validateImport`), never here.
object AppExport {
    const val FORMAT = "exponential-app/1"
    const val SUFFIX = ".app.json"
    const val MIME = "application/json"

    object Limits {
        const val IMPORT_MAX_BYTES = 4_194_304
        const val RECORDS_MAX = 2000
        const val ISSUES_SHOWN = 8
    }

    object Copy {
        const val EXPORT = "Export"
        const val EXPORT_TITLE = "Export {name}"
        const val INCLUDE_RECORDS = "Include records"
        const val IMPORT = "Import…"
        const val IMPORT_TITLE = "Import an app"
        const val IMPORT_PLACEHOLDER = "Paste an app file or a package"
        const val CHOOSE_FILE = "Choose file"
        const val IMPORT_ACTION = "Import"
        const val IMPORT_NOTE = "Imported"
        const val MORE_ISSUES = "and {n} more"
    }

    /** The app's identity as the file carries it. */
    data class App(val name: String, val icon: String?, val color: String?, val description: String?)

    /** One synced record (only `kind` + `data` reach the file). */
    data class Record(val id: String, val kind: String, val data: JsonValue, val createdAt: String)

    /** One server issue (`apps.validateImport`). */
    data class Issue(val path: String, val message: String)

    fun exportTitle(name: String): String = Copy.EXPORT_TITLE.replace("{name}", name)

    fun moreIssues(n: Int): String = Copy.MORE_ISSUES.replace("{n}", n.toString())

    /** `path: message`, the row the import step lists. */
    fun issueLine(issue: Issue): String = "${issue.path}: ${issue.message}"

    /** `working-hours` → `working-hours.app.json`. */
    fun appExportFileName(slug: String): String = "$slug$SUFFIX"

    /** The file: the identity, the head package and, when switched on, every
     *  record as `{kind, data}` oldest first (createdAt, then id). Never ids,
     *  authors, times or grants; no `records` key when there are none. */
    fun buildAppExport(app: App, pkg: JsonValue, records: List<Record>, includeRecords: Boolean): JsonValue {
        val file = LinkedHashMap<String, JsonValue>()
        file["format"] = JsonValue.Str(FORMAT)
        file["app"] = JsonValue.Obj(
            linkedMapOf(
                "name" to JsonValue.Str(app.name),
                "icon" to (app.icon?.let { JsonValue.Str(it) } ?: JsonValue.Null),
                "color" to (app.color?.let { JsonValue.Str(it) } ?: JsonValue.Null),
                "description" to JsonValue.Str(app.description.orEmpty()),
            ),
        )
        file["package"] = pkg
        if (includeRecords && records.isNotEmpty()) {
            val sorted = records.sortedWith(
                compareBy<Record> { WireTimestamps.parseEpochMs(it.createdAt) ?: 0L }.thenBy { it.id },
            )
            file["records"] = JsonValue.Arr(
                sorted.map { JsonValue.Obj(linkedMapOf("kind" to JsonValue.Str(it.kind), "data" to it.data)) },
            )
        }
        return JsonValue.Obj(file)
    }

    /** The file's text: two-space JSON (keys in written order) and a
     *  trailing newline, like web's `appExportText`. */
    fun appExportText(file: JsonValue): String = StringBuilder().also { pretty(file, it, "") }.append('\n').toString()

    private fun pretty(v: JsonValue, sb: StringBuilder, indent: String) {
        when (v) {
            is JsonValue.Arr -> if (v.v.isEmpty()) {
                sb.append("[]")
            } else {
                val inner = "$indent  "
                sb.append("[\n")
                v.v.forEachIndexed { i, item ->
                    sb.append(inner)
                    pretty(item, sb, inner)
                    if (i < v.v.size - 1) sb.append(',')
                    sb.append('\n')
                }
                sb.append(indent).append(']')
            }
            is JsonValue.Obj -> if (v.v.isEmpty()) {
                sb.append("{}")
            } else {
                val inner = "$indent  "
                sb.append("{\n")
                v.v.entries.forEachIndexed { i, (key, item) ->
                    sb.append(inner).append(JsonValue.Str(key).json).append(": ")
                    pretty(item, sb, inner)
                    if (i < v.v.size - 1) sb.append(',')
                    sb.append('\n')
                }
                sb.append(indent).append('}')
            }
            else -> sb.append(v.json)
        }
    }
}
