package com.exponential.app.ui.apps

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.exponential.app.data.api.AppUpstreamMapping
import com.exponential.app.domain.AppExport
import com.exponential.app.domain.AppTemplates
import com.exponential.app.domain.Apps
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.OptionGroup
import com.exponential.app.ui.components.SwitchRow
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.SheetPrimaryAction
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.parseColor
import com.exponential.app.ui.theme.TextEmphasis

// VAPP-110 (fixtures/apps.json THE LIST, web `components/apps/apps-list.tsx`):
// the Apps TAB = the band whose `New app` pill opens THE NEW MENU (VAPP-242:
// New app, Import…), then one row per app by name (glyph, name, the version
// pill, the `…` menu: Edit app, Export, Delete app); Exponential ships no
// built-in apps. VAPP-242: a row RUNS the app (THE RUN VIEW); the editor sits
// behind `…` › Edit app. VAPP-176: New app = `apps.create {teamId}` (a DRAFT)
// and its editor at once, no form; Import… opens the import step as a sheet.

/** An app's glyph: its icon in its colour (the board set), muted without one. */
@Composable
fun AppGlyph(icon: String?, color: String?, size: Dp = 16.dp, modifier: Modifier = Modifier) {
    val glyph = icon?.let { ExpIcons.byName(it) } ?: ExpIcons.navApps
    val tint = color?.let { runCatching { parseColor(it) }.getOrNull() }
        ?: MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    Icon(glyph, contentDescription = null, tint = tint, modifier = modifier.size(size))
}

/** The version pill = `Apps.listPill` (eApps V1, #VAPP-210: the PUBLISHED
 *  version `v{n}`, or the muted `Draft` while nothing is published). */
@Composable
fun AppVersionPill(app: AppListEntry) {
    val versioned = app.publishedVersion > 0
    GlassPill(
        Apps.listPill(app.versions),
        size = PillSize.Sm,
        contentColor = if (versioned) null else MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
    )
}

/** VAPP-121 / VAPP-176: Import… = its own sheet, the import step: the
 *  monospace field, Choose file, the server's issues and Import. VAPP-234
 *  (fixtures/app-templates.json): the templates step above it (a row =
 *  [onTemplate] at once, the sample-records switch). #VAPP-254: an owner's
 *  Accounts group under the step (`upstreamPickerCases`). */
@Composable
fun AppImportSheet(
    busy: Boolean,
    onDismiss: () -> Unit,
    onValidate: suspend (String) -> AppImportCheck?,
    onImport: (text: String, upstreamMappings: List<AppUpstreamMapping>) -> Unit,
    onTemplate: (template: String, includeRecords: Boolean) -> Unit = { _, _ -> },
) {
    // Plain remember: an import may be up to 4 MB, far past the Binder limit for saved state.
    var text by remember { mutableStateOf("") }
    val check = rememberImportCheck(text, onValidate)
    val issues = check?.issues
    var picks by remember { mutableStateOf(mapOf<String, String>()) }
    GlassSheet(
        title = AppExport.Copy.IMPORT_TITLE,
        onDismiss = onDismiss,
        modifier = Modifier.testTag("app-import-dialog"),
        primaryAction = SheetPrimaryAction(
            label = AppExport.Copy.IMPORT_ACTION,
            onClick = { if (check != null && issues?.isEmpty() == true) onImport(text, importUpstreamMappings(check.upstreams, picks)) },
            enabled = issues?.isEmpty() == true && !busy,
            loading = busy,
            testTag = "app-import-confirm",
        ),
    ) {
        // #VAPP-254: the Accounts group grows the step past the sheet's cap.
        Column(Modifier.fillMaxWidth().weight(1f, fill = false).verticalScroll(rememberScrollState())) {
            AppTemplatesStep(busy = busy, onTemplate = onTemplate)
            Spacer(Modifier.height(12.dp))
            AppImportStep(text = text, onTextChange = { text = it }, issues = issues)
            if (check != null && issues?.isEmpty() == true && check.upstreams.isNotEmpty() && check.servers.isNotEmpty()) {
                Spacer(Modifier.height(12.dp))
                AppImportUpstreams(check, picks, enabled = !busy) { upstream, value -> picks = picks + (upstream to value) }
            }
            Spacer(Modifier.height(8.dp))
        }
    }
}

/** VAPP-234: the templates step: the band, one row per signed template
 *  (glyph, name over the muted description), the sample-records switch. */
@Composable
fun AppTemplatesStep(busy: Boolean, onTemplate: (template: String, includeRecords: Boolean) -> Unit) {
    var withRecords by rememberSaveable { mutableStateOf(false) }
    Column(modifier = Modifier.fillMaxWidth()) {
        SectionHeader(AppTemplates.Copy.TITLE, modifier = Modifier.padding(horizontal = 16.dp))
        OptionGroup {
            AppTemplates.TEMPLATES.forEachIndexed { index, template ->
                if (index > 0) GroupDivider()
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                    modifier = Modifier
                        .fillMaxWidth()
                        .testTag("${AppTemplates.TestIds.ROW}-${template.id}")
                        .clickable(enabled = !busy) { onTemplate(template.id, withRecords) }
                        .padding(horizontal = 16.dp, vertical = 10.dp),
                ) {
                    AppGlyph(template.icon, template.color)
                    Column(modifier = Modifier.weight(1f)) {
                        Text(
                            template.name,
                            fontWeight = FontWeight.Medium,
                            style = MaterialTheme.typography.bodyMedium,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                        Text(
                            template.description,
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                }
            }
        }
        Spacer(Modifier.height(12.dp))
        OptionGroup {
            SwitchRow(
                title = AppTemplates.Copy.INCLUDE_RECORDS,
                checked = withRecords,
                onCheckedChange = { withRecords = it },
                enabled = !busy,
                modifier = Modifier.testTag(AppTemplates.TestIds.RECORDS),
            )
        }
    }
}
