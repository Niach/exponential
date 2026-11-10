package com.exponential.app.ui.apps

import android.content.Context
import com.exponential.app.ui.theme.ExpMono
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
import androidx.compose.material3.Icon
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.size
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
import androidx.compose.ui.Alignment
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import at.exponential.ui.json.JsonValue
import com.exponential.app.data.api.AppUpstreamMapping
import com.exponential.app.data.api.AppServerDto
import com.exponential.app.data.api.AppImportUpstreamDto
import com.exponential.app.data.db.AppRecordEntity
import com.exponential.app.domain.AppExport
import com.exponential.app.domain.Apps
import com.exponential.app.ui.components.FailedAction
import com.exponential.app.ui.components.FailedActionToast
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassSubmitButton
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.OptionGroup
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.SubmitSize
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.SheetPrimaryAction
import com.exponential.app.ui.components.SwitchRow
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.components.picker.PickerMode
import com.exponential.app.ui.components.picker.PickerItem
import com.exponential.app.ui.components.picker.Picker
import com.exponential.app.ui.components.picker.InlinePickerRow
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
            enabled = app.headPackageJson != null && !exporting,
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
    // eApps V1: the export carries the head (the draft, else the published package).
    val pkg = JsonValue.parse(app.headPackageJson ?: error("no version yet"))
    val file = AppExport.buildAppExport(
        app = AppExport.App(app.name, app.icon, app.color, app.description),
        pkg = pkg,
        records = records.map { r ->
            AppExport.Record(
                id = r.id,
                kind = r.kind,
                data = r.data?.takeIf { it.isNotBlank() }?.let(JsonValue::parse) ?: JsonValue.Obj(emptyMap()),
                createdAt = r.createdAt,
                author = r.authorId ?: r.createdBy,
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

/** #VAPP-254: what the server answered for a file: its issues, the upstreams it
 *  declares and the servers an owner may pick for them (empty for a member). */
data class AppImportCheck(
    val issues: List<AppExport.Issue>,
    val upstreams: List<AppImportUpstreamDto> = emptyList(),
    val servers: List<AppServerDto> = emptyList(),
)

/** #VAPP-254: the owner's [picks] (name → option value) for `apps.import`:
 *  only names the file still declares, never `none`. */
internal fun importUpstreamMappings(upstreams: List<AppImportUpstreamDto>, picks: Map<String, String>): List<AppUpstreamMapping> =
    upstreams.mapNotNull { upstream ->
        picks[upstream.name]?.takeUnless { it == Apps.UPSTREAM_PICKER_NONE }?.let { AppUpstreamMapping(upstream.name, it) }
    }

/** #VAPP-254 (`upstreamPickerCases`): which team server fills an upstream the
 *  app declares: the Settings Accounts rows and the import step's. THE picker
 *  with the Data tab's chevron pill as its trigger; no servers = nothing. */
@Composable
internal fun UpstreamPickerPill(
    name: String,
    serverId: String?,
    servers: List<AppServerDto>,
    testTag: String,
    enabled: Boolean = true,
    onPick: (String) -> Unit,
) {
    val picker = Apps.upstreamPicker(name, serverId, servers.map { it.id to it.name }) ?: return
    Picker(
        items = picker.options.map { PickerItem(it.value, it.label) },
        mode = PickerMode.Single,
        value = setOf(picker.value),
        onChange = { next -> next.firstOrNull()?.takeIf { it != picker.value }?.let(onPick) },
        enabled = enabled,
    ) { open ->
        GlassPill(
            picker.options.firstOrNull { it.value == picker.value }?.label ?: Apps.AccountsCopy.NOT_SET_UP,
            size = PillSize.Sm,
            onClick = open,
            enabled = enabled,
            trailing = { Icon(ExpIcons.uiChevronDown, contentDescription = null, modifier = Modifier.size(14.dp)) },
            shrinkLabel = true,
            modifier = Modifier.widthIn(max = 128.dp).testTag(testTag),
        )
    }
}

/** #VAPP-254: the import step's Accounts group: one row per upstream the file
 *  declares, its name and the server picker from the exact-name default. */
@Composable
internal fun AppImportUpstreams(
    check: AppImportCheck,
    picks: Map<String, String>,
    enabled: Boolean,
    onPick: (upstream: String, value: String) -> Unit,
) {
    Column(Modifier.fillMaxWidth().testTag(Apps.TestIds.IMPORT_UPSTREAMS)) {
        SectionHeader(Apps.AccountsCopy.TITLE, modifier = Modifier.padding(horizontal = 16.dp))
        OptionGroup {
            check.upstreams.forEachIndexed { index, upstream ->
                if (index > 0) GroupDivider()
                val picked = picks[upstream.name]
                InlinePickerRow(
                    modifier = Modifier.testTag("${Apps.TestIds.IMPORT_UPSTREAM}-${upstream.name}"),
                    rowModifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    scroll = false,
                ) {
                    Text(
                        upstream.name,
                        style = MaterialTheme.typography.bodyMedium,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f),
                    )
                    UpstreamPickerPill(
                        name = upstream.name,
                        serverId = if (picked == null) upstream.serverId else picked.takeUnless { it == Apps.UPSTREAM_PICKER_NONE },
                        servers = check.servers,
                        testTag = "${Apps.TestIds.IMPORT_UPSTREAM}-${upstream.name}-picker",
                        enabled = enabled,
                        onPick = { onPick(upstream.name, it) },
                    )
                }
            }
        }
    }
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
            minLines = 6,
            maxLines = 12,
            textStyle = MaterialTheme.typography.bodySmall.copy(fontFamily = ExpMono),
            modifier = Modifier.fillMaxWidth().testTag("app-import-text"),
        )
        Row {
            // EXP-1676: web's outline `Button size="sm"` (h-8), not a chip.
            GlassSubmitButton(
                AppExport.Copy.CHOOSE_FILE,
                onClick = { chooser.launch(arrayOf(AppExport.MIME)) },
                size = SubmitSize.Small,
                outline = true,
                modifier = Modifier.testTag("app-import-file"),
            )
        }
        if (!issues.isNullOrEmpty()) {
            Column(
                verticalArrangement = Arrangement.spacedBy(4.dp),
                modifier = Modifier.fillMaxWidth().testTag("app-import-issues"),
            ) {
                val style = MaterialTheme.typography.bodySmall.copy(fontFamily = ExpMono)
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
internal fun readImportFile(context: Context, uri: Uri): String {
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
fun rememberImportCheck(text: String, validate: suspend (String) -> AppImportCheck?): AppImportCheck? {
    var check by remember { mutableStateOf<AppImportCheck?>(null) }
    LaunchedEffect(text) {
        if (text.isBlank()) {
            check = null
            return@LaunchedEffect
        }
        delay(VALIDATE_DEBOUNCE_MS)
        check = validate(text)
    }
    return check
}
