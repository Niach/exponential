package com.exponential.app.ui.apps

import android.net.Uri
import com.exponential.app.ui.theme.ExpMono
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import at.exponential.ui.ffi.validateRecordJson
import at.exponential.ui.json.JsonValue
import com.exponential.app.domain.AppRecords
import com.exponential.app.domain.AppRecords.CsvCopy
import com.exponential.app.domain.AppRecords.CsvImport
import com.exponential.app.domain.AppRecords.CsvTestIds
import com.exponential.app.domain.AppRecords.DataCopy
import com.exponential.app.ui.components.FailedAction
import com.exponential.app.ui.components.FailedActionToast
import com.exponential.app.ui.components.FieldError
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassSubmitButton
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.OptionGroup
import com.exponential.app.ui.components.PickerRow
import com.exponential.app.ui.components.SubmitSize
import com.exponential.app.ui.components.SheetHeight
import com.exponential.app.ui.components.SheetPrimaryAction
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassGroup
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

// VAPP-228 (X2, fixtures/app-records.json `csvImport`; web
// `components/apps/app-csv-import-dialog.tsx`): the Data tab's CSV import on
// a phone, the app import sheet's pattern: paste or choose a CSV, map the
// columns onto the kind's fields (auto-matched; an OWNER's Author first),
// preview the first rows with their issues (the eUI schema check appended to
// a row with none), then ONE `apps.records.batch` (`via: import`) of the
// ready rows; the toast's Undo deletes them again.

/** The Skip / Me option of a column picker. */
private const val NONE = ""

private val CSV_MIMES = arrayOf("text/csv", "text/comma-separated-values", "text/tab-separated-values", "text/plain")

/** The host's schema check (eUI `validateRecord`, the server's own) as
 *  `row {row}: {path}: {message}`; the core unavailable = no issues. */
private fun schemaIssues(schema: JsonValue?, record: AppRecords.CsvRecord): List<String> {
    if (schema == null) return emptyList()
    val out = runCatching { JsonValue.parse(validateRecordJson(schema.json, JsonValue.Obj(record.data).json)) }.getOrNull()
    return out?.array.orEmpty().map { issue ->
        CsvCopy.ISSUE.replace("{row}", record.row.toString())
            .replace("{field}", (issue["path"]?.string ?: "").removePrefix("/"))
            .replace("{message}", issue["message"]?.string ?: "")
    }
}

@Composable
fun AppCsvImportSheet(
    kind: String,
    schema: JsonValue?,
    viewModel: AppEditorViewModel,
    onDismiss: () -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val me by viewModel.me.collectAsStateWithLifecycle()
    val role by viewModel.myRole.collectAsStateWithLifecycle()
    val options by viewModel.authorOptions.collectAsStateWithLifecycle()
    val users by viewModel.users.collectAsStateWithLifecycle()
    val busy by viewModel.csvImporting.collectAsStateWithLifecycle()
    val error by viewModel.csvError.collectAsStateWithLifecycle()
    val isOwner = role == com.exponential.app.domain.DomainContract.teamRoleOwner
    // An OWNER's team members for the Author column (null = no Author pick).
    val members = remember(isOwner, options, users) {
        if (!isOwner) null else options.map { AppRecords.CsvMember(it.id, users[it.id]?.name ?: "", users[it.id]?.email ?: "") }
    }

    // Plain remember: a pasted CSV may be far past the Binder limit for saved state.
    var text by remember { mutableStateOf("") }
    var failure by remember { mutableStateOf<FailedAction?>(null) }
    FailedActionToast(failure, onShown = { failure = null })
    LaunchedEffect(Unit) { viewModel.clearCsvError() }

    val fields = remember(schema) { AppRecords.dataColumns(schema) }
    val required = remember(schema) { schema?.get("required")?.array.orEmpty().mapNotNull { it.string } }
    val table = remember(text) { AppRecords.csvParse(text) }
    // A new header = a new first pick.
    var mapping by remember(table.header, fields) { mutableStateOf(AppRecords.csvAutoMap(fields, table.header)) }
    var authorColumn by remember(table.header, members != null) { mutableStateOf(if (members != null) AppRecords.csvAuthorColumn(table.header) else null) }

    val records = remember(schema, table, mapping, authorColumn, members, me) {
        val author = if (members != null && me != null) AppRecords.CsvAuthor(authorColumn, me!!, members) else null
        AppRecords.csvRecords(schema, table.header, table.rows, mapping, author).map { record ->
            if (record.issues.isNotEmpty()) record
            else schemaIssues(schema, record).let { issues -> if (issues.isEmpty()) record else record.copy(issues = issues) }
        }
    }
    val ready = records.filter { it.issues.isEmpty() }
    val tooMany = records.size > CsvImport.MAX_ROWS

    val chooser = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri: Uri? ->
        if (uri == null) return@rememberLauncherForActivityResult
        scope.launch {
            try {
                text = withContext(Dispatchers.IO) { readImportFile(context, uri) }
                viewModel.clearCsvError()
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                failure = FailedAction("open", "the file", t.message)
            }
        }
    }

    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    val issueStyle = MaterialTheme.typography.labelSmall.copy(fontFamily = ExpMono)

    GlassSheet(
        title = CsvCopy.TITLE.replace("{kind}", kind),
        onDismiss = { if (!busy) onDismiss() },
        height = SheetHeight.Full,
        modifier = Modifier.testTag(CsvTestIds.DIALOG),
        primaryAction = SheetPrimaryAction(
            label = CsvCopy.IMPORT_ACTION.replace("{n}", ready.size.toString()),
            onClick = { if (!tooMany && ready.isNotEmpty()) viewModel.importRecords(kind, ready, onDismiss) },
            enabled = !tooMany && ready.isNotEmpty() && !busy,
            loading = busy,
            testTag = CsvTestIds.CONFIRM,
        ),
    ) {
        Column(
            verticalArrangement = Arrangement.spacedBy(12.dp),
            modifier = Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(vertical = 4.dp),
        ) {
            Column(
                verticalArrangement = Arrangement.spacedBy(12.dp),
                modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp),
            ) {
                GlassTextField(
                    value = text,
                    onValueChange = {
                        text = it
                        viewModel.clearCsvError()
                    },
                    placeholder = CsvCopy.PLACEHOLDER,
                    minLines = 6,
                    maxLines = 10,
                    textStyle = MaterialTheme.typography.bodySmall.copy(fontFamily = ExpMono),
                    modifier = Modifier.fillMaxWidth().testTag(CsvTestIds.TEXT),
                )
                Row {
                    // EXP-1676: web's outline `Button size="sm"` (h-8), not a chip.
                    GlassSubmitButton(
                        CsvCopy.CHOOSE_FILE,
                        onClick = { chooser.launch(CSV_MIMES) },
                        size = SubmitSize.Small,
                        outline = true,
                        modifier = Modifier.testTag(CsvTestIds.FILE),
                    )
                }
            }
            if (table.header.isNotEmpty()) {
                val header = table.header
                val columnOptions = header.indices.map { it.toString() }
                fun columnLabel(option: String, none: String): String = option.toIntOrNull()?.let { header.getOrNull(it) } ?: none
                Column(Modifier.fillMaxWidth().testTag(CsvTestIds.MAP)) {
                    OptionGroup {
                        if (members != null) {
                            val selected = authorColumn?.toString() ?: NONE
                            PickerRow(
                                label = DataCopy.AUTHOR,
                                value = columnLabel(selected, CsvCopy.ME),
                                options = listOf(NONE) + columnOptions,
                                selected = selected,
                                optionLabel = { columnLabel(it, CsvCopy.ME) },
                                onSelect = { authorColumn = it.toIntOrNull() },
                                modifier = Modifier.testTag(CsvTestIds.AUTHOR),
                            )
                        }
                        fields.forEachIndexed { index, name ->
                            if (index > 0 || members != null) GroupDivider()
                            val selected = mapping[name]?.toString() ?: NONE
                            PickerRow(
                                label = name,
                                value = columnLabel(selected, CsvCopy.SKIP),
                                options = columnOptions + NONE,
                                selected = selected,
                                optionLabel = { columnLabel(it, CsvCopy.SKIP) },
                                onSelect = { pick -> mapping = mapping + (name to pick.toIntOrNull()) },
                                labelHint = if (name in required) null else DataCopy.OPTIONAL,
                                modifier = Modifier.testTag("app-csv-field-$name"),
                            )
                        }
                    }
                }
            }
            if (text.isNotBlank() && records.isEmpty()) {
                Text(CsvCopy.NO_ROWS, color = muted, style = MaterialTheme.typography.bodySmall, modifier = Modifier.padding(horizontal = 20.dp))
            }
            if (records.isNotEmpty()) {
                Column(
                    verticalArrangement = Arrangement.spacedBy(6.dp),
                    modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp).testTag(CsvTestIds.PREVIEW),
                ) {
                    Column(Modifier.fillMaxWidth().glassGroup()) {
                        records.take(CsvImport.PREVIEW_ROWS).forEachIndexed { index, record ->
                            if (index > 0) GroupDivider()
                            val title = fields.firstOrNull()?.let { AppRecords.cellText(record.data[it]) }.orEmpty()
                            val subtitle = fields.drop(1).map { AppRecords.cellText(record.data[it]) }.filter { it.isNotEmpty() }.joinToString(" · ")
                            Column(Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 8.dp)) {
                                Text(
                                    title.ifEmpty { DataCopy.EMPTY_TITLE },
                                    fontWeight = if (title.isEmpty()) null else FontWeight.Medium,
                                    color = if (title.isEmpty()) muted else MaterialTheme.colorScheme.onSurface,
                                    style = MaterialTheme.typography.bodyMedium,
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                )
                                if (subtitle.isNotEmpty()) {
                                    Text(subtitle, color = muted, style = MaterialTheme.typography.labelSmall, maxLines = 1, overflow = TextOverflow.Ellipsis)
                                }
                                if (record.issues.isNotEmpty()) {
                                    Spacer(Modifier.height(4.dp))
                                    val tone = MaterialTheme.colorScheme.error
                                    record.issues.take(CsvImport.ISSUES_SHOWN).forEach { Text(it, style = issueStyle, color = tone) }
                                    if (record.issues.size > CsvImport.ISSUES_SHOWN) {
                                        Text(CsvCopy.MORE_ISSUES.replace("{n}", (record.issues.size - CsvImport.ISSUES_SHOWN).toString()), style = issueStyle, color = tone)
                                    }
                                }
                            }
                        }
                    }
                    Text(AppRecords.csvSummary(records), color = muted, style = MaterialTheme.typography.labelSmall, modifier = Modifier.padding(horizontal = 4.dp))
                    if (tooMany) {
                        Text(
                            CsvCopy.TOO_MANY.replace("{max}", CsvImport.MAX_ROWS.toString()),
                            color = MaterialTheme.colorScheme.error,
                            style = MaterialTheme.typography.bodySmall,
                            modifier = Modifier.padding(horizontal = 4.dp),
                        )
                    }
                }
            }
            FieldError(error, modifier = Modifier.padding(horizontal = 20.dp).testTag(CsvTestIds.ERROR))
            Spacer(Modifier.height(8.dp))
        }
    }
}
