package com.exponential.app.ui.apps

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import at.exponential.ui.json.JsonValue
import at.exponential.ui.primitives.EmptyStateSize
import com.exponential.app.data.db.AppRecordEntity
import com.exponential.app.domain.AppRecords
import com.exponential.app.domain.AppRecords.Controls
import com.exponential.app.domain.AppRecords.DataCopy
import com.exponential.app.domain.AppRecords.DataTestIds
import com.exponential.app.domain.Apps
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.FieldError
import com.exponential.app.ui.components.GlassAlert
import com.exponential.app.ui.components.GlassAlertAction
import com.exponential.app.ui.components.GlassDropdownMenu
import com.exponential.app.ui.components.GlassMenuItem
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.OptionGroup
import com.exponential.app.ui.components.PickerRow
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.SheetPrimaryAction
import com.exponential.app.ui.components.SwitchRow
import com.exponential.app.ui.components.TextFieldRow
import com.exponential.app.ui.components.picker.Picker
import com.exponential.app.ui.components.picker.PickerItem
import com.exponential.app.ui.components.picker.PickerMode
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.relativeTime
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassGroup

// VAPP-178 THE DATA TAB on a phone (fixtures/app-records.json `dataTab`,
// PHONES; web `components/apps/app-data-tab.tsx`): the head package's record
// KINDS in a select (`{kind} · {n}`, the first picked on open) + Add record,
// then ONE grouped list of the picked kind's rows (`updatedAt` desc): title =
// the first column's cell, subtitle = the other non-empty cells joined ` · `,
// trailing the relative `updated` + the `…` menu Edit / Delete. Add / Edit =
// a sheet with one labelled row per column (`formFields`); Save =
// `apps.records.upsert` as the viewer, a BAD_REQUEST inline under the form,
// anything else the failed-action toast. Delete asks first.

/** The open form: a new row of [kind], or the edit of [row]. */
private data class DataFormTarget(val kind: String, val row: AppRecords.Row?)

@Composable
fun AppDataTab(
    app: AppListEntry,
    records: List<AppRecordEntity>,
    viewModel: AppEditorViewModel,
    modifier: Modifier = Modifier,
) {
    val manifest = remember(app.packageJson) { app.packageJson?.let { runCatching { JsonValue.parse(it) }.getOrNull() } }
    val rows = remember(records) { records.map(AppRecords::row) }
    val kinds = remember(manifest, rows) { AppRecords.dataKinds(manifest, rows.map { it.kind }) }
    var picked by rememberSaveable { mutableStateOf<String?>(null) }
    val kind = picked?.takeIf { p -> kinds.any { it.kind == p } } ?: kinds.firstOrNull()?.kind
    val schema = kind?.let { AppRecords.dataSchema(manifest, it) }
    val columns = remember(schema) { AppRecords.dataColumns(schema) }
    val shown = remember(rows, kind) { if (kind == null) emptyList() else AppRecords.dataRows(rows, kind) }
    var form by remember { mutableStateOf<DataFormTarget?>(null) }
    var deleting by remember { mutableStateOf<String?>(null) }
    val busyDeleting by viewModel.recordDeleting.collectAsStateWithLifecycle()

    Column(modifier = modifier.fillMaxSize().testTag(Apps.TestIds.DATA)) {
        if (kind == null) {
            EmptyState(
                title = DataCopy.NO_KINDS,
                icon = ExpIcons.navApps,
                size = EmptyStateSize.List,
                modifier = Modifier.padding(16.dp),
            )
            return@Column
        }
        // TOOLBAR: the kind select + Add record (compact).
        Row(
            verticalAlignment = Alignment.CenterVertically,
            modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
        ) {
            val current = kinds.first { it.kind == kind }
            Picker(
                items = kinds.map { PickerItem(it.kind, kindOption(it)) },
                mode = PickerMode.Single,
                value = setOf(kind),
                onChange = { next -> next.firstOrNull()?.let { picked = it } },
            ) { open ->
                GlassPill(
                    kindOption(current),
                    size = PillSize.Sm,
                    onClick = open,
                    trailing = { Icon(ExpIcons.uiChevronDown, contentDescription = null, modifier = Modifier.size(14.dp)) },
                    modifier = Modifier.testTag(DataTestIds.KIND),
                )
            }
            Spacer(Modifier.weight(1f))
            GlassPill(
                DataCopy.ADD_RECORD,
                size = PillSize.Sm,
                icon = ExpIcons.uiAdd,
                onClick = {
                    viewModel.clearRecordError()
                    form = DataFormTarget(kind, null)
                },
                modifier = Modifier.testTag(DataTestIds.ADD),
            )
        }
        if (shown.isEmpty()) {
            EmptyState(
                title = DataCopy.EMPTY,
                icon = ExpIcons.navApps,
                size = EmptyStateSize.List,
                modifier = Modifier.padding(16.dp),
            )
            return@Column
        }
        Column(
            Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(start = 16.dp, end = 16.dp, top = 4.dp, bottom = 24.dp),
        ) {
            Column(Modifier.fillMaxWidth().glassGroup()) {
                shown.forEachIndexed { index, row ->
                    if (index > 0) GroupDivider()
                    DataRow(
                        row = row,
                        columns = columns,
                        onEdit = {
                            viewModel.clearRecordError()
                            form = DataFormTarget(kind, row)
                        },
                        onDelete = { deleting = row.id },
                    )
                }
            }
        }
    }

    form?.let { target ->
        DataFormSheet(
            target = target,
            schema = AppRecords.dataSchema(manifest, target.kind),
            viewModel = viewModel,
            onDismiss = {
                form = null
                viewModel.clearRecordError()
            },
        )
    }
    deleting?.let { recordId ->
        GlassAlert(
            title = DataCopy.DELETE_TITLE,
            body = DataCopy.DELETE_BODY,
            onDismiss = { if (!busyDeleting) deleting = null },
            trailing = listOf(
                GlassAlertAction(DataCopy.CANCEL, onClick = { deleting = null }),
                GlassAlertAction(
                    DataCopy.DELETE,
                    destructive = true,
                    loading = busyDeleting,
                    testTag = "app-data-delete-confirm",
                    onClick = { viewModel.deleteRecord(recordId) { deleting = null } },
                ),
            ),
            defaultAction = 0,
        )
    }
}

private fun kindOption(kind: AppRecords.DataKind): String =
    DataCopy.KIND_OPTION.replace("{kind}", kind.kind).replace("{n}", kind.count.toString())

/** One phone row: title + muted subtitle, the relative `updated`, the `…` menu. */
@Composable
private fun DataRow(row: AppRecords.Row, columns: List<String>, onEdit: () -> Unit, onDelete: () -> Unit) {
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    val subtitle = AppRecords.rowSubtitle(columns, row)
    var menu by remember { mutableStateOf(false) }
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        modifier = Modifier
            .fillMaxWidth()
            .testTag(DataTestIds.ROW)
            .padding(start = 12.dp, end = 4.dp, top = 8.dp, bottom = 8.dp),
    ) {
        Column(Modifier.weight(1f)) {
            Text(
                AppRecords.rowTitle(columns, row),
                fontWeight = FontWeight.Medium,
                style = MaterialTheme.typography.bodyMedium,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            if (subtitle.isNotEmpty()) {
                Text(
                    subtitle,
                    color = muted,
                    style = MaterialTheme.typography.labelSmall,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        Text(relativeTime(row.updatedAt), color = muted, style = MaterialTheme.typography.labelSmall, maxLines = 1)
        Box {
            CircleIconButton(
                ExpIcons.uiMore,
                contentDescription = "Record actions",
                onClick = { menu = true },
                borderless = true,
                modifier = Modifier.testTag(DataTestIds.MENU),
            )
            GlassDropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                GlassMenuItem(
                    leadingIcon = { Icon(ExpIcons.uiEdit, contentDescription = null) },
                    text = { Text(DataCopy.EDIT) },
                    onClick = {
                        menu = false
                        onEdit()
                    },
                    modifier = Modifier.testTag("app-data-row-edit"),
                )
                GlassMenuItem(
                    leadingIcon = { Icon(ExpIcons.uiDelete, contentDescription = null) },
                    text = { Text(DataCopy.DELETE) },
                    destructive = true,
                    onClick = {
                        menu = false
                        onDelete()
                    },
                    modifier = Modifier.testTag("app-data-row-delete"),
                )
            }
        }
    }
}

/** ADD / EDIT: a sheet titled `New {kind}` / `Edit {kind}`, one labelled row
 *  per column (`formFields`), Save pinned at the bottom; the server's
 *  BAD_REQUEST reads inline under the rows. */
@Composable
private fun DataFormSheet(
    target: DataFormTarget,
    schema: JsonValue?,
    viewModel: AppEditorViewModel,
    onDismiss: () -> Unit,
) {
    val fields = remember(schema) { AppRecords.formFields(schema) }
    val inputs = remember(target) {
        mutableStateMapOf<String, Any>().apply {
            for (field in fields) put(field.name, AppRecords.formInput(field.control, target.row?.data?.get(field.name)))
        }
    }
    val saving by viewModel.recordSaving.collectAsStateWithLifecycle()
    val error by viewModel.recordError.collectAsStateWithLifecycle()
    val editing = target.row != null
    val title = (if (editing) DataCopy.EDIT_TITLE else DataCopy.NEW_TITLE).replace("{kind}", target.kind)
    LaunchedEffect(target) { viewModel.clearRecordError() }
    GlassSheet(
        title = title,
        onDismiss = onDismiss,
        primaryAction = SheetPrimaryAction(
            label = DataCopy.SAVE,
            loading = saving,
            enabled = !saving,
            testTag = DataTestIds.SAVE,
            onClick = {
                viewModel.saveRecord(target.kind, AppRecords.formData(schema, inputs.toMap(), editing), target.row?.id, onDone = onDismiss)
            },
        ),
    ) {
        Column(
            verticalArrangement = Arrangement.spacedBy(8.dp),
            modifier = Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(vertical = 4.dp)
                .testTag(DataTestIds.FORM),
        ) {
            OptionGroup {
                fields.forEachIndexed { index, field ->
                    if (index > 0) GroupDivider()
                    val hint = if (field.required) null else DataCopy.OPTIONAL
                    when (field.control) {
                        Controls.SWITCH -> SwitchRow(
                            title = field.name,
                            checked = inputs[field.name] == true,
                            onCheckedChange = { inputs[field.name] = it },
                            labelHint = hint,
                            modifier = Modifier.testTag("app-data-field-${field.name}"),
                        )
                        Controls.SELECT -> {
                            val options = (if (field.required) emptyList() else listOf("")) + field.options.orEmpty()
                            val value = inputs[field.name] as? String ?: ""
                            PickerRow(
                                label = field.name,
                                value = value.ifEmpty { if (field.required) "" else DataCopy.NONE },
                                options = options,
                                selected = value,
                                optionLabel = { it.ifEmpty { DataCopy.NONE } },
                                onSelect = { inputs[field.name] = it },
                                labelHint = hint,
                                modifier = Modifier.testTag("app-data-field-${field.name}"),
                            )
                        }
                        else -> TextFieldRow(
                            label = field.name,
                            value = inputs[field.name] as? String ?: "",
                            onValueChange = { inputs[field.name] = it },
                            placeholder = when (field.control) {
                                Controls.LIST -> DataCopy.LIST_HINT
                                else -> field.placeholder
                            },
                            keyboardOptions = if (field.control == Controls.NUMBER) {
                                KeyboardOptions(keyboardType = KeyboardType.Decimal)
                            } else {
                                KeyboardOptions.Default
                            },
                            labelHint = hint,
                            modifier = Modifier.testTag("app-data-field-${field.name}"),
                        )
                    }
                }
            }
            FieldError(error, modifier = Modifier.padding(horizontal = 20.dp).testTag(DataTestIds.ERROR))
            Spacer(Modifier.width(1.dp))
        }
    }
}
