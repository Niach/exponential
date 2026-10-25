package com.exponential.app.ui.apps

import android.content.Context
import android.content.Intent
import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import at.exponential.ui.json.JsonValue
import com.exponential.app.data.db.AppRecordEntity
import com.exponential.app.domain.AppExport
import com.exponential.app.ui.components.FailedAction
import com.exponential.app.ui.components.FailedActionToast
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.components.OptionGroup
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.SheetPrimaryAction
import com.exponential.app.ui.components.SwitchRow
import com.exponential.app.ui.issue.fileProviderUri
import java.io.File
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

// VAPP-121 (fixtures/app-export.json, web `components/apps/app-export-dialog.tsx`
// + the import step of `app-form-dialog.tsx`): Export = ONE sheet titled
// `copy.exportTitle`, the "Include records" switch row (only when the app has
// records, off by default) and the Export button, which writes
// `<slug>.app.json` under the cache's `app-exports/` (the FileProvider path)
// and hands it to the system share intent. Import = New app's second step:
// paste or choose a file, the SERVER lists its issues, Import creates the app.

private const val EXPORT_DIR = "app-exports"
private const val VALIDATE_DEBOUNCE_MS = 300L

/** Export [app] (records off by default; the switch only when there are any). */
@Composable
fun AppExportSheet(app: AppListEntry, records: List<AppRecordEntity>, onDismiss: () -> Unit) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var includeRecords by rememberSaveable(app.id) { mutableStateOf(false) }
    var exporting by remember { mutableStateOf(false) }
    var failure by remember { mutableStateOf<FailedAction?>(null) }
    FailedActionToast(failure, onShown = { failure = null })

    fun export() {
        if (exporting) return
        exporting = true
        scope.launch {
            try {
                val file = withContext(Dispatchers.IO) { writeAppExport(context, app, records, includeRecords) }
                shareAppExport(context, file)
                onDismiss()
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                failure = FailedAction("export", "the app", t.message)
            } finally {
                exporting = false
            }
        }
    }

    GlassSheet(
        title = AppExport.exportTitle(app.name),
        onDismiss = onDismiss,
        modifier = Modifier.testTag("app-export-dialog"),
        primaryAction = SheetPrimaryAction(
            label = AppExport.Copy.EXPORT,
            onClick = ::export,
            enabled = app.packageJson != null && !exporting,
            loading = exporting,
            testTag = "app-export-confirm",
        ),
    ) {
        if (records.isNotEmpty()) {
            OptionGroup {
                SwitchRow(
                    title = AppExport.Copy.INCLUDE_RECORDS,
                    checked = includeRecords,
                    onCheckedChange = { includeRecords = it },
                    modifier = Modifier.testTag("app-export-records"),
                )
            }
            Spacer(Modifier.height(8.dp))
        }
    }
}

/** The file for [app] under the cache's export dir (the latest export only). */
private fun writeAppExport(context: Context, app: AppListEntry, records: List<AppRecordEntity>, includeRecords: Boolean): File {
    val pkg = JsonValue.parse(app.packageJson ?: error("no version yet"))
    val file = AppExport.buildAppExport(
        app = AppExport.App(app.name, app.icon, app.color, app.description),
        pkg = pkg,
        records = records.map { r ->
            AppExport.Record(
                id = r.id,
                kind = r.kind,
                data = r.data?.takeIf { it.isNotBlank() }?.let(JsonValue::parse) ?: JsonValue.Obj(emptyMap()),
                createdAt = r.createdAt,
            )
        },
        includeRecords = includeRecords,
    )
    val dir = File(context.cacheDir, EXPORT_DIR).apply { mkdirs() }
    // Only the latest export stays in the cache (the share intent holds its grant).
    dir.listFiles()?.forEach { it.delete() }
    return File(dir, AppExport.appExportFileName(app.slug)).apply { writeText(AppExport.appExportText(file)) }
}

/** The system share intent with a FileProvider URI (the chooser carries the grant). */
private fun shareAppExport(context: Context, file: File) {
    val uri = fileProviderUri(context, file)
    val send = Intent(Intent.ACTION_SEND).apply {
        type = AppExport.MIME
        putExtra(Intent.EXTRA_STREAM, uri)
        putExtra(Intent.EXTRA_TITLE, file.name)
        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
    }
    context.startActivity(
        Intent.createChooser(send, file.name).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION),
    )
}

/** The import step's body: the monospace field, Choose file and the
 *  server's issues. [issues] null = not checked yet (or the check failed). */
@Composable
fun AppImportStep(
    text: String,
    onTextChange: (String) -> Unit,
    issues: List<AppExport.Issue>?,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var failure by remember { mutableStateOf<FailedAction?>(null) }
    FailedActionToast(failure, onShown = { failure = null })
    val chooser = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri: Uri? ->
        if (uri == null) return@rememberLauncherForActivityResult
        scope.launch {
            try {
                onTextChange(withContext(Dispatchers.IO) { readImportFile(context, uri) })
            } catch (t: Throwable) {
                if (t is CancellationException) throw t
                failure = FailedAction("open", "the file", t.message)
            }
        }
    }
    Column(
        verticalArrangement = Arrangement.spacedBy(12.dp),
        modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp),
    ) {
        GlassTextField(
            value = text,
            onValueChange = onTextChange,
            placeholder = AppExport.Copy.IMPORT_PLACEHOLDER,
            minLines = 8,
            maxLines = 12,
            textStyle = MaterialTheme.typography.bodySmall.copy(fontFamily = FontFamily.Monospace),
            modifier = Modifier.fillMaxWidth().testTag("app-import-text"),
        )
        Row {
            GlassPill(
                AppExport.Copy.CHOOSE_FILE,
                size = PillSize.Sm,
                onClick = { chooser.launch(arrayOf(AppExport.MIME)) },
                modifier = Modifier.testTag("app-import-file"),
            )
        }
        if (!issues.isNullOrEmpty()) {
            Column(
                verticalArrangement = Arrangement.spacedBy(4.dp),
                modifier = Modifier.fillMaxWidth().testTag("app-import-issues"),
            ) {
                val style = MaterialTheme.typography.bodySmall.copy(fontFamily = FontFamily.Monospace)
                val tone = MaterialTheme.colorScheme.error
                issues.take(AppExport.Limits.ISSUES_SHOWN).forEach { Text(AppExport.issueLine(it), style = style, color = tone) }
                if (issues.size > AppExport.Limits.ISSUES_SHOWN) {
                    Text(AppExport.moreIssues(issues.size - AppExport.Limits.ISSUES_SHOWN), style = style, color = tone)
                }
            }
        }
    }
}

/** The text of a chosen file, capped at `limits.importMaxBytes`. */
private fun readImportFile(context: Context, uri: Uri): String {
    val stream = context.contentResolver.openInputStream(uri) ?: error("unreadable")
    val bytes = stream.use { it.readNBytesCompat(AppExport.Limits.IMPORT_MAX_BYTES + 1) }
    if (bytes.size > AppExport.Limits.IMPORT_MAX_BYTES) error("the file is over ${AppExport.Limits.IMPORT_MAX_BYTES / (1024 * 1024)} MB")
    return bytes.toString(Charsets.UTF_8)
}

private fun java.io.InputStream.readNBytesCompat(limit: Int): ByteArray {
    val out = java.io.ByteArrayOutputStream()
    val buf = ByteArray(8192)
    while (out.size() < limit) {
        val n = read(buf, 0, minOf(buf.size, limit - out.size()))
        if (n < 0) break
        out.write(buf, 0, n)
    }
    return out.toByteArray()
}

/** Validate [text] against the server, debounced; blank = no issues listed. */
@Composable
fun rememberImportIssues(text: String, validate: suspend (String) -> List<AppExport.Issue>?): List<AppExport.Issue>? {
    var issues by remember { mutableStateOf<List<AppExport.Issue>?>(null) }
    LaunchedEffect(text) {
        if (text.isBlank()) {
            issues = null
            return@LaunchedEffect
        }
        delay(VALIDATE_DEBOUNCE_MS)
        issues = validate(text)
    }
    return issues
}
