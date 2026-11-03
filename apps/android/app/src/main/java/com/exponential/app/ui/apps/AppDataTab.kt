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
import com.exponential.app.domain.AppRecords.HistoryCopy
import com.exponential.app.domain.Apps
import com.exponential.app.domain.DomainContract
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
import com.exponential.app.ui.components.LocalToaster
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.OptionGroup
import com.exponential.app.ui.components.PickerRow
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.SheetPrimaryAction
import com.exponential.app.ui.components.SwitchRow
import com.exponential.app.ui.components.TextFieldRow
import com.exponential.app.ui.components.userDisplayName
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
// anything else the failed-action toast. Delete asks first. VAPP-188: a
// team OWNER's form leads with the Author picker (`formAuthor`); Save sends
// `author` only when the pick differs from the record's author / the viewer.
// (Phones show no `by` cell, so `recordByline` has no phone site.)
// VAPP-215 (`history`): every write toasts Undo for 10 s; Edit lists the
// record's History (who · when · what) with Restore on older rows.

/** The open form: a new row of [kind], or the edit of [row]. */
private data class DataFormTarget(val kind: String, val row: AppRecords.Row?)

@Composable
fun AppDataTab(
    app: AppListEntry,
    records: List<AppRecordEntity>,
    viewModel: AppEditorViewModel,
    modifier: Modifier = Modifier,
) {
    // The head package's manifest (eApps V1: the draft, else the published one).
    val manifest = remember(app.headPackageJson) { app.headPackageJson?.let { runCatching { JsonValue.parse(it) }.getOrNull() } }
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
    // VAPP-187: a row I may not change keeps its `…`, disabled.
    val me by viewModel.me.collectAsStateWithLifecycle()
    val role by viewModel.myRole.collectAsStateWithLifecycle()
    // eApps R3: the kind's policy under my app access (eUI record-policy.json).
    val access by viewModel.myAccess.collectAsStateWithLifecycle()
    fun may(op: String, k: String, row: AppRecords.Row? = null) =
        AppRecords.Policy.allowed(AppRecords.Policy.of(manifest, k), op, me, role, access, row)
    // VAPP-215: an add / save / delete / restore toasts Undo (10 s).
    val toaster = LocalToaster.current
    LaunchedEffect(viewModel) {
        viewModel.undo.collect { u -> toaster.recordUndo(u.write, u.eventIds) { viewModel.undoRecords(u.eventIds, u.via) } }
    }

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
                enabled = may("create", kind),
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
                        canEdit = may("update", kind, row),
                        canDelete = may("delete", kind, row),
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
private fun DataRow(row: AppRecords.Row, columns: List<String>, canEdit: Boolean, canDelete: Boolean, onEdit: () -> Unit, onDelete: () -> Unit) {
    val canChange = canEdit || canDelete
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
                enabled = canChange,
                borderless = true,
                modifier = Modifier.testTag(DataTestIds.MENU),
            )
            GlassDropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                if (canEdit) GlassMenuItem(
                    leadingIcon = { Icon(ExpIcons.uiEdit, contentDescription = null) },
                    text = { Text(DataCopy.EDIT) },
                    onClick = {
                        menu = false
                        onEdit()
                    },
                    modifier = Modifier.testTag("app-data-row-edit"),
                )
                if (canDelete) GlassMenuItem(
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
    // VAPP-188: a team OWNER's first field = the Author picker (members by
    // name, required); non-owners never see it nor send `author`.
    val me by viewModel.me.collectAsStateWithLifecycle()
    val role by viewModel.myRole.collectAsStateWithLifecycle()
    val members by viewModel.authorOptions.collectAsStateWithLifecycle()
    val users by viewModel.users.collectAsStateWithLifecycle()
    val isOwner = role == DomainContract.teamRoleOwner
    var authorPick by remember(target) { mutableStateOf<String?>(null) }
    val author = authorPick ?: AppRecords.formAuthor(me, target.row, null).default
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
                val send = if (isOwner) AppRecords.formAuthor(me, target.row, author).send else null
                viewModel.saveRecord(
                    target.kind,
                    AppRecords.formData(schema, inputs.toMap(), editing),
                    target.row?.id,
                    author = send,
                    onDone = onDismiss,
                )
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
                if (isOwner) {
                    val names = members.associate { it.id to it.name }
                    PickerRow(
                        label = DataCopy.AUTHOR,
                        value = author?.let { names[it] ?: userDisplayName(users[it], it) } ?: "",
                        options = members.map { it.id },
                        selected = author,
                        optionLabel = { id -> names[id] ?: userDisplayName(users[id], id) },
                        onSelect = { authorPick = it },
                        modifier = Modifier.testTag("app-data-field-author"),
                    )
                }
                fields.forEachIndexed { index, field ->
                    if (index > 0 || isOwner) GroupDivider()
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
            target.row?.let { row -> RecordHistory(row.id, viewModel, onRestored = onDismiss) }
            Spacer(Modifier.width(1.dp))
        }
    }
}

/** VAPP-215: a record's History (read on open): newest first, who · when
 *  under the label, Restore on every older row but deletes. */
@Composable
private fun RecordHistory(recordId: String, viewModel: AppEditorViewModel, onRestored: () -> Unit) {
    var events by remember(recordId) { mutableStateOf<List<AppRecords.RecordEvent>?>(null) }
    LaunchedEffect(recordId) { events = viewModel.recordHistory(recordId) }
    val restoring by viewModel.recordRestoring.collectAsStateWithLifecycle()
    val users by viewModel.users.collectAsStateWithLifecycle()
    val names = remember(users) { users.mapValues { (id, user) -> userDisplayName(user, id) } }
    val rows = events?.let { AppRecords.historyRows(it, names, Apps.Copy.BY_AGENT) }.orEmpty()
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    Column(Modifier.fillMaxWidth().padding(top = 8.dp).testTag(DataTestIds.HISTORY)) {
        SectionHeader(HistoryCopy.HISTORY, loading = events == null, modifier = Modifier.padding(horizontal = 16.dp))
        if (events != null && rows.isEmpty()) {
            EmptyState(
                title = HistoryCopy.EMPTY,
                size = EmptyStateSize.List,
                modifier = Modifier.padding(horizontal = 16.dp),
            )
        } else if (rows.isNotEmpty()) {
            OptionGroup {
                rows.forEachIndexed { index, row ->
                    if (index > 0) GroupDivider()
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                        modifier = Modifier
                            .fillMaxWidth()
                            .testTag(DataTestIds.HISTORY_ROW)
                            .padding(horizontal = 12.dp, vertical = 8.dp),
                    ) {
                        Column(Modifier.weight(1f)) {
                            Text(row.label, style = MaterialTheme.typography.bodyMedium, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            Text(
                                "${row.who} · ${relativeTime(row.at)}",
                                color = muted,
                                style = MaterialTheme.typography.labelSmall,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                        }
                        if (row.restorable) {
                            GlassPill(
                                HistoryCopy.RESTORE,
                                size = PillSize.Sm,
                                loading = restoring == row.id,
                                enabled = restoring == null,
                                onClick = { viewModel.restoreRecord(recordId, row.id, onRestored) },
                                modifier = Modifier.testTag(DataTestIds.RESTORE),
                            )
                        }
                    }
                }
            }
        }
    }
}
