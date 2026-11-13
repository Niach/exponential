package com.exponential.app.ui.repo

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import at.exponential.ui.primitives.EmptyStateSize
import com.exponential.app.data.api.RepoCiSettings
import com.exponential.app.domain.PrChecks
import com.exponential.app.domain.RepoCi
import com.exponential.app.domain.RepoPage
import com.exponential.app.ui.ci.CiSurface
import com.exponential.app.ui.ci.CiSurfaceHandle
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.GlassAlert
import com.exponential.app.ui.components.GlassAlertAction
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.PillMode
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.SheetPrimaryAction
import com.exponential.app.ui.components.SkeletonRows
import com.exponential.app.ui.components.SwitchRow
import com.exponential.app.ui.components.TextFieldRow
import com.exponential.app.ui.issue.relativeTime
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.ExpMono
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassGroup
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.add
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.put
import kotlinx.serialization.json.putJsonArray
import kotlinx.serialization.json.putJsonObject

// #EXP-1617 E4 part 4 (fixtures/repo-ci.json): the repo page's Runs tab and
// the CI section of its Settings tab, phone layout, as the genUI trees web
// builds (`apps/web/src/lib/repo-ci-view.ts`: `repoRunsTree`,
// `repoCiSectionTree`), painted by [CiSurface] like the run page and the
// runner section. The two overlays stay E3's: Delete's AlertDialog =
// GlassAlert, Add secret's phone Drawer = GlassSheet ([AddSecretSheet]).

private const val OPEN_RUN = "repo-run-open"
private const val REQUIRED_PATH = "/required"
private const val MAX_REQUIRED = 64

private fun node(
    id: String,
    component: String,
    props: (kotlinx.serialization.json.JsonObjectBuilder.() -> Unit)? = null,
    children: List<JsonElement>? = null,
    extra: kotlinx.serialization.json.JsonObjectBuilder.() -> Unit = {},
): JsonObject = buildJsonObject {
    put("id", id)
    put("component", component)
    if (props != null) putJsonObject("props", props)
    extra()
    if (children != null) put("children", JsonArray(children))
}

private fun iconTone(tone: PrChecks.Tone): String = when (tone) {
    PrChecks.Tone.Muted -> "neutral"
    PrChecks.Tone.Green -> "success"
    PrChecks.Tone.Red -> "danger"
}

/** PHONES: one `Row` per run (leading glyph tinted, `#n`, the name,
 *  `ref · sha7 · trigger`, the duration); a press opens the run page. */
internal fun repoRunsTree(rows: List<RepoCi.RunRow>): JsonObject = node(
    RepoCi.TestIds.RUNS,
    "Section",
    null,
    rows.map { row ->
        node(
            "${RepoCi.TestIds.RUN_ROW}.${row.id}",
            "Row",
            {
                put("title", row.title)
                put("identifier", row.identifier)
                put("subtitle", row.subtitle)
                row.duration?.let { put("meta", it) }
                put("pressable", true)
            },
        ) {
            putJsonObject("slots") {
                put(
                    "leading",
                    node("${RepoCi.TestIds.RUN_ROW}.${row.id}.glyph", "Icon", {
                        put("name", row.glyph.wire)
                        put("tone", iconTone(row.tone))
                        put("size", "sm")
                    }),
                )
            }
            putJsonObject("on") {
                putJsonObject("press") {
                    putJsonObject("event") {
                        put("name", OPEN_RUN)
                        putJsonObject("context") { put("run", row.id) }
                    }
                }
            }
        }
    },
)

/** Events the CI section's tree fires (web `REPO_CI_EVENTS`). */
private object CiEvents {
    const val REQUIRED = "repo-ci-required"
    const val GITHUB_STATUSES = "repo-ci-github-statuses"
    const val ACTIONS_RUNNERS = "repo-ci-actions-runners"
    const val DELETE_SECRET = "repo-ci-delete-secret"
    const val ADD_SECRET = "repo-ci-add-secret"
}

/** repo-page.json `copy.checks[state].tone` → the Chip's dot token (web `DOT`). */
private fun dotToken(state: String): String = when (RepoPage.Copy.CHECKS[state]?.tone) {
    "green" -> "\$color.success"
    "amber" -> "\$color.warning"
    "red" -> "\$color.destructive"
    else -> "\$color.mutedForeground"
}

private fun kotlinx.serialization.json.JsonObjectBuilder.onEvent(event: String, name: String, context: (kotlinx.serialization.json.JsonObjectBuilder.() -> Unit)? = null) {
    putJsonObject("on") {
        putJsonObject(event) {
            putJsonObject("event") {
                put("name", name)
                if (context != null) putJsonObject("context", context)
            }
        }
    }
}

/**
 * The CI section, PHONE layout, as web's `repoCiSectionTree` builds it
 * (`apps/web/src/lib/repo-ci-view.ts`, same ids, props and order): a
 * `Section` CI → the pipelines (`Row` + trailing `Chip{dot}` with its label;
 * none = muted Text) → `SettingsRow` Required checks holding the `ChipInput`
 * → `Heading` h4 Secrets, the secret `Row`s (meta = relative time, trailing
 * a ghost sm `Button` Delete for managers; none = muted Text), the outline
 * sm `Button` Add secret (managers) → per `switches(provider)` a
 * `SettingsRow` + `Switch`. [secrets] null = still loading (that block waits).
 */
internal fun repoCiSectionTree(
    provider: String?,
    pipelines: List<RepoCi.PipelineRow>,
    suggestions: List<String>,
    secrets: List<com.exponential.app.data.api.RepoCiSecret>?,
    githubStatuses: Boolean,
    actionsRunners: Boolean,
    manager: Boolean,
): JsonObject {
    val ids = RepoCi.TestIds
    val disabled = !manager
    val children = mutableListOf<JsonElement>()
    children += if (pipelines.isEmpty()) {
        node(ids.PIPELINES, "Text", { put("text", RepoCi.Copy.Ci.NO_PIPELINE); put("variant", "muted") })
    } else {
        node(
            ids.PIPELINES,
            "Stack",
            { put("direction", "vertical"); put("gap", "none"); put("align", "stretch") },
            pipelines.map { p ->
                node("${ids.PIPELINES}.${p.runId}", "Row", { put("title", p.name) }) {
                    putJsonObject("slots") {
                        put(
                            "trailing",
                            node("${ids.PIPELINES}.${p.runId}.state", "Chip", {
                                put("label", RepoPage.Copy.CHECKS[p.state]?.label ?: p.state)
                                put("dot", dotToken(p.state))
                            }),
                        )
                    }
                }
            },
        )
    }
    children += node(
        "repo-ci-required-row",
        "SettingsRow",
        {
            put("label", RepoCi.Copy.Ci.REQUIRED_LABEL)
            put("description", RepoCi.Copy.Ci.REQUIRED_DESCRIPTION)
            put("stackBelow", "md")
        },
        listOf(
            node(ids.REQUIRED, "ChipInput", {
                put("name", ids.REQUIRED)
                putJsonObject("values") { put("@path", REQUIRED_PATH) }
                put("placeholder", RepoCi.Copy.Ci.REQUIRED_PLACEHOLDER)
                putJsonArray("suggestions") { suggestions.forEach { s -> add(buildJsonObject { put("label", s); put("value", s) }) } }
                put("disabled", disabled)
            }) { onEvent("change", CiEvents.REQUIRED) },
        ),
    )
    if (secrets != null) {
        children += node("repo-ci-secrets-heading", "Heading", { put("text", RepoCi.Copy.Secrets.TITLE); put("level", "h4") })
        children += if (secrets.isEmpty()) {
            node(ids.SECRETS, "Text", { put("text", RepoCi.Copy.Secrets.EMPTY); put("variant", "muted") })
        } else {
            node(
                ids.SECRETS,
                "Stack",
                { put("direction", "vertical"); put("gap", "none"); put("align", "stretch") },
                secrets.map { s ->
                    node("${ids.SECRETS}.${s.name}", "Row", {
                        put("title", s.name)
                        putJsonObject("meta") {
                            put("@call", "formatRelativeTime")
                            putJsonObject("args") { put("value", s.updatedAt) }
                        }
                    }) {
                        if (manager) {
                            putJsonObject("slots") {
                                put(
                                    "trailing",
                                    node("${ids.SECRETS}.${s.name}.delete", "Button", {
                                        put("label", RepoCi.Copy.Secrets.DELETE)
                                        put("variant", "ghost")
                                        put("size", "sm")
                                    }) { onEvent("press", CiEvents.DELETE_SECRET) { put("name", s.name) } },
                                )
                            }
                        }
                    }
                },
            )
        }
        if (manager) {
            // #EXP-1813: a Button in a stretch column fills it; in a row it
            // keeps its own width, like web's Dialog trigger.
            children += node(
                "${ids.ADD_SECRET}-row",
                "Stack",
                { put("direction", "horizontal") },
                children = listOf(
                    node(ids.ADD_SECRET, "Button", {
                        put("label", RepoCi.Copy.Secrets.ADD)
                        put("variant", "outline")
                        put("size", "sm")
                    }) { onEvent("press", CiEvents.ADD_SECRET) },
                ),
            )
        }
    }
    for (switch in RepoCi.switches(provider)) {
        val copy = RepoCi.Copy.SWITCHES.getValue(switch)
        val (id, event, checked) = when (switch) {
            RepoCi.Switch.GithubStatuses -> Triple(ids.GITHUB_STATUSES, CiEvents.GITHUB_STATUSES, githubStatuses)
            RepoCi.Switch.ActionsRunners -> Triple(ids.ACTIONS_RUNNERS, CiEvents.ACTIONS_RUNNERS, actionsRunners)
        }
        children += node(
            "$id-row",
            "SettingsRow",
            { put("label", copy.label); put("description", copy.description) },
            listOf(
                node(id, "Switch", {
                    put("label", "")
                    put("name", id)
                    put("checked", checked)
                    put("disabled", disabled)
                }) { onEvent("change", event) },
            ),
        )
    }
    return node(ids.CI, "Section", { put("title", RepoCi.Copy.Ci.SECTION) }, children)
}

/** RUNS: `runRows` newest first; none = the EmptyState. */
@Composable
internal fun RepoRunsTab(rows: List<RepoCi.RunRow>, onOpenRun: (String) -> Unit) {
    if (rows.isEmpty()) {
        EmptyState(title = RepoPage.Copy.Runs.EMPTY, size = EmptyStateSize.List)
        return
    }
    val tree = remember(rows) { repoRunsTree(rows) }
    CiSurface(surfaceId = "repo-runs", tree = tree, modifier = Modifier.fillMaxWidth().testTag(RepoCi.TestIds.RUNS)) { name, context, _ ->
        if (name == OPEN_RUN) (context?.get("run") as? JsonPrimitive)?.contentOrNull?.let(onOpenRun)
    }
}

/**
 * SETTINGS › CI ([repoCiSectionTree], painted by [CiSurface]); Delete opens
 * the AlertDialog (GlassAlert), Add secret the phones' Drawer
 * ([AddSecretSheet]). [canManage] false = every control disabled, the
 * secret actions hidden. [ci] null = still loading.
 */
@Composable
internal fun RepoCiSection(
    provider: String?,
    runs: List<RepoCi.Run>,
    jobs: List<RepoCi.Job>,
    ci: RepoCiSettings?,
    canManage: Boolean,
    onSetRequired: (List<String>) -> Unit,
    onSetSwitch: (RepoCi.Switch, Boolean) -> Unit,
    onSaveSecret: (name: String, value: String, onDone: (Boolean) -> Unit) -> Unit,
    onDeleteSecret: (String) -> Unit,
) {
    var deleting by remember { mutableStateOf<String?>(null) }
    var adding by remember { mutableStateOf(false) }
    val required = ci?.config?.requiredChecks.orEmpty()
    val pipelines = remember(runs) { RepoCi.pipelineRows(runs) }
    val suggestions = remember(runs, jobs, required) { RepoCi.requiredSuggestions(runs, jobs, required) }
    val githubStatuses = ci?.config?.githubStatuses == true
    val actionsRunners = ci?.config?.actionsRunners == true
    val tree = remember(provider, pipelines, suggestions, ci?.secrets, githubStatuses, actionsRunners, canManage) {
        repoCiSectionTree(provider, pipelines, suggestions, ci?.secrets, githubStatuses, actionsRunners, canManage)
    }
    val handle = remember { CiSurfaceHandle() }
    fun values(list: List<String>) = JsonArray(list.map(::JsonPrimitive))
    // The stored list wins (a dropped duplicate leaves the field).
    LaunchedEffect(required) { handle.setData(REQUIRED_PATH, values(required)) }
    CiSurface(
        surfaceId = "repo-ci",
        tree = tree,
        modifier = Modifier.fillMaxWidth().testTag(RepoCi.TestIds.CI),
        data = mapOf("required" to values(required)),
        handle = handle,
    ) { name, context, payload ->
        fun checked() = (payload?.get("checked") as? JsonPrimitive)?.takeIf { !it.isString }?.content?.toBooleanStrictOrNull()
        when (name) {
            CiEvents.REQUIRED -> {
                val entries = (payload?.get("values") as? JsonArray)?.mapNotNull { (it as? JsonPrimitive)?.contentOrNull?.trim() }
                    ?: return@CiSurface
                val next = entries.filter { it.isNotEmpty() && it.length <= 255 }.distinct().take(MAX_REQUIRED)
                handle.setData(REQUIRED_PATH, values(next))
                if (canManage && next != required) onSetRequired(next)
            }
            CiEvents.GITHUB_STATUSES -> checked()?.let { if (canManage && it != githubStatuses) onSetSwitch(RepoCi.Switch.GithubStatuses, it) }
            CiEvents.ACTIONS_RUNNERS -> checked()?.let { if (canManage && it != actionsRunners) onSetSwitch(RepoCi.Switch.ActionsRunners, it) }
            CiEvents.DELETE_SECRET -> if (canManage) deleting = (context?.get("name") as? JsonPrimitive)?.contentOrNull
            CiEvents.ADD_SECRET -> if (canManage) adding = true
        }
    }
    deleting?.let { name ->
        GlassAlert(
            title = RepoCi.Copy.Secrets.DELETE_TITLE,
            trailing = listOf(
                GlassAlertAction(RepoCi.Copy.Secrets.CANCEL, onClick = { deleting = null }),
                GlassAlertAction(
                    RepoCi.Copy.Secrets.DELETE_CONFIRM,
                    onClick = {
                        deleting = null
                        onDeleteSecret(name)
                    },
                    destructive = true,
                    testTag = "repo-ci-secret-delete-confirm",
                ),
            ),
            onDismiss = { deleting = null },
        )
    }
    if (adding) AddSecretSheet(onSave = onSaveSecret, onDismiss = { adding = false })
}

/** Add secret, the phones' Drawer: Name (validated by `secretName`,
 *  invalid = inline), Value (a password field), Save as the sheet's action. */
@Composable
internal fun AddSecretSheet(
    onSave: (name: String, value: String, onDone: (Boolean) -> Unit) -> Unit,
    onDismiss: () -> Unit,
    initialName: String = "",
) {
    var name by remember { mutableStateOf(initialName) }
    var value by remember { mutableStateOf("") }
    var pending by remember { mutableStateOf(false) }
    val validName = RepoCi.secretName(name)
    val valid = validName && value.isNotEmpty()
    val save = {
        if (valid && !pending) {
            pending = true
            onSave(name, value) { ok ->
                pending = false
                if (ok) onDismiss()
            }
        }
    }
    GlassSheet(
        title = RepoCi.Copy.Secrets.ADD,
        onDismiss = onDismiss,
        primaryAction = SheetPrimaryAction(
            label = RepoCi.Copy.Secrets.SAVE,
            onClick = { save() },
            enabled = valid,
            loading = pending,
            testTag = "repo-ci-secret-save",
        ),
    ) {
        Column(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 20.dp).testTag("repo-ci-secret-form"),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Column(Modifier.fillMaxWidth().glassGroup()) {
                TextFieldRow(
                    label = RepoCi.Copy.Secrets.NAME,
                    value = name,
                    onValueChange = { name = it.trim().take(128) },
                    placeholder = "NPM_TOKEN",
                    keyboardOptions = KeyboardOptions(
                        capitalization = KeyboardCapitalization.Characters,
                        autoCorrectEnabled = false,
                        imeAction = ImeAction.Next,
                    ),
                    textStyle = TextStyle(fontFamily = ExpMono),
                    modifier = Modifier.testTag("repo-ci-secret-name"),
                )
                GroupDivider()
                TextFieldRow(
                    label = RepoCi.Copy.Secrets.VALUE,
                    value = value,
                    onValueChange = { value = it },
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, imeAction = ImeAction.Done),
                    keyboardActions = KeyboardActions(onDone = { save() }),
                    visualTransformation = PasswordVisualTransformation(),
                    modifier = Modifier.testTag("repo-ci-secret-value"),
                )
            }
            if (name.isNotEmpty() && !validName) {
                Text(
                    RepoCi.Copy.Secrets.INVALID,
                    style = MaterialTheme.typography.bodySmall,
                    color = DesignTokens.Semantic.Red,
                    modifier = Modifier.padding(horizontal = 16.dp).testTag("repo-ci-secret-invalid"),
                )
            }
        }
    }
}
