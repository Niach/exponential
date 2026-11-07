package com.exponential.app.domain

import at.exponential.ui.json.JsonValue
import java.net.URLEncoder
import java.text.Normalizer

// VAPP-110: Exponential apps, the Android mirror of the pure reference
// `packages/domain-contract/src/apps.ts` (fixtures/apps.json is the spec;
// `AppsTest` replays its `slugifyCases`, `versionLabelCases`,
// `sortAppsCases`, `publishCases`, `automationRowCases` and the VAPP-218 `screen*Cases` and locks the copy below to the fixture byte for byte).
object Apps {
    object Copy {
        const val NAV = "Apps"
        const val TITLE = "Apps"
        const val NEW_APP = "New app"
        // VAPP-176: New app = a DRAFT with this name (the agent renames it).
        const val UNTITLED = "Untitled app"
        const val IMPORT = "Import…"
        const val VERSION = "v{n}"
        const val NO_VERSION = "Draft"
        const val VERSIONS = "Versions"
        const val HEAD = "Current"
        const val NO_NOTE = "No note"
        const val BY_AGENT = "the editor session"
        const val VIEWING = "Viewing v{n}"
        const val RESTORE = "Restore"
        const val RESTORED = "Restored v{n} as v{m}"
        const val RESTORED_NOTE = "Restored v{n}"
        // eApps V1 (#VAPP-210): Publish, the published row's badge, its toast
        // and the server's refusal while nothing exists.
        const val PUBLISH = "Publish"
        const val PUBLISHED = "Published"
        const val PUBLISHED_TOAST = "Published v{n}"
        const val PUBLISH_NOTHING = "Nothing to publish yet"
        // eApps R8 (#VAPP-233, `migrationCases`): a version row's record sweep.
        const val MIGRATING = "Migrating records"
        const val MIGRATING_OF = "Migrating records {done} of {total}"
        const val MIGRATION_FAILED = "Records not migrated: {error}"
        // #VAPP-256 (`rollbackCases`).
        const val ROLLBACK_TITLE = "Roll back to v{n}?"
        const val ROLLBACK_BODY = "Rollback doesn't reverse migrated data. Records stay in the newer shape, and v{n} can refuse edits to them."
        const val EMPTY = "No apps yet"
        const val CHAT_EMPTY = "Describe the app you want. The editor session builds it and the preview follows."
        const val PREVIEW_EMPTY = "No version yet"
        const val VERSIONS_EMPTY = "No versions yet"
        const val COMPOSER_PLACEHOLDER = "Ask for a change…"
        // VAPP-176: a draft's empty chat: the starters (a tap sends it).
        const val DRAFT_PLACEHOLDER = "Describe your app…"
        const val STARTER_TITLE = "What should this app do?"
        val STARTERS = listOf("Log working hours", "Release checklist", "Admin console for an API", "Team dashboard")
        const val NO_DEVICE = "Start a device to edit this app"
        const val NAME_PLACEHOLDER = "Name"
        const val DESCRIPTION_PLACEHOLDER = "What is it for? (optional)"
        const val DELETE_TITLE = "Delete {name}?"
        const val DELETE_BODY = "Its versions go with it. This cannot be undone."
        const val DELETE = "Delete"
        // VAPP-177: the phone editor's `…` menu (`editorTabs.menu`).
        const val MENU_VERSIONS = "Versions"
        const val MENU_SETTINGS = "Settings"
        const val MENU_EXPORT = "Export"
        const val MENU_DELETE = "Delete app"
        // VAPP-242: the list row's / the run view's `…` first item (→ THE EDITOR).
        const val MENU_EDIT = "Edit app"
        // VAPP-242: THE RUN VIEW of an app with nothing published.
        const val RUN_EMPTY = "Not published yet"
        const val NEEDS_UPDATE = "This app needs an update"
        const val TAB_CHAT = "Chat"
        const val TAB_PREVIEW = "Preview"
        // VAPP-178: the Data tab (app-records.json `dataTab`).
        const val TAB_DATA = "Data"
        const val TAB_VERSIONS = "Versions"
        const val TAB_SETTINGS = "Settings"
        // VAPP-177: a Versions row's ghost button (switches to that version).
        const val PREVIEW = "Preview"
        // VAPP-122: the app's records count (Settings → Apps → the app).
        const val RECORDS = "{n} records"
        const val RECORDS_ONE = "1 record"
        const val RECORDS_OF = "{n} of {max} records"
        const val RECORDS_NONE = "No records"
    }

    /** #VAPP-220: the Settings tab's Automations group (`copy.automations`). */
    object AutomationsCopy {
        const val TITLE = "Automations"
        const val SCHEDULE = "Schedule {cron}"
        const val SCHEDULE_ZONE = "Schedule {cron} ({tz})"
        const val CREATE = "When a {kind} is created"
        const val UPDATE = "When a {kind} is updated"
        const val DELETE = "When a {kind} is deleted"
        const val EVENT = "On {event}"
        const val WEBHOOK = "On webhook {name}"
        const val PAUSED = "Paused"
        const val FAILED = "Failed"
        const val OK = "OK"
        const val NEVER = "Not run yet"
        /** #VAPP-243: who a run acts as, or why it is paused. */
        const val RUNS_AS = "Runs as {name}"
        /** #VAPP-255: the runs the rate cap dropped. */
        const val DROPPED = "{n} runs dropped in 24 h"
        const val DROPPED_ONE = "1 run dropped in 24 h"
        val PAUSED_REASONS: Map<String, String> = mapOf(
            "sponsorGone" to "Its sponsor is no longer on the team",
            "sponsorReadOnly" to "Its sponsor has read-only access",
            "sponsorNotAllowed" to "Only an owner can make it run as someone else",
        )
    }

    /** #VAPP-238: the Settings tab's Webhooks group (`copy.webhooks`). */
    object WebhooksCopy {
        const val TITLE = "Webhooks"
        const val READY = "Ready"
        const val UNSIGNED = "No secret"
        const val COPY_URL = "Copy URL"
        const val COPIED = "Copied"
        const val MINT = "Create secret"
        const val ROTATE = "Rotate secret"
        const val SECRET = "New secret"
        const val COPY_SECRET = "Copy secret"
    }

    /** #VAPP-243: the Settings tab's Accounts group (`copy.accounts`). */
    object AccountsCopy {
        const val TITLE = "Accounts"
        const val CONNECT = "Connect"
        const val RECONNECT = "Reconnect"
        const val CONNECTED = "Connected"
        const val NOT_SET_UP = "Not set up"
        /** An owner's action on web and desktop (phones add no servers). */
        const val ADD_SERVER = "Add server"
        /** #VAPP-254: an Accounts row's second line. */
        const val USES_SERVER = "Uses {server}"
    }

    /** #VAPP-243: the Settings tab's MCP tools group (`copy.mcpTools`). */
    object McpToolsCopy {
        const val TITLE = "MCP tools"
        const val READ = "Read"
        const val WRITE = "Write"
    }

    /** #VAPP-243: the Settings tab's Record access group (`copy.policies`). */
    object PoliciesCopy {
        const val TITLE = "Record access"
        val OPS: Map<String, String> = mapOf("read" to "Read", "create" to "Add", "update" to "Change", "delete" to "Delete")
        val LEVELS: Map<String, String> = mapOf("member" to "Members", "author" to "Author", "owners" to "Owners")
    }

    object Preview {
        const val RADIUS = 12
        const val PADDING = 16
        const val SURFACE_ID = "app"
    }

    object TestIds {
        const val LIST = "apps-list"
        const val ROW = "apps-row"
        const val NEW_APP = "apps-new"
        const val EDITOR = "app-editor"
        const val CHAT = "app-editor-chat"
        const val PREVIEW = "app-editor-preview"
        const val VERSIONS = "app-editor-versions"
        const val VERSION_ROW = "app-version-row"
        const val TABS = "app-editor-tabs"
        const val COMPOSER = "app-editor-composer"
        const val VIEWING = "app-editor-viewing"
        // VAPP-176: the list band's Import pill + a draft's starter pills.
        const val IMPORT_APP = "apps-import"
        const val STARTER = "app-editor-starter"
        // VAPP-177: the phone header's `…`, the Settings body, a Versions
        // row's Preview / Restore.
        const val MENU = "app-editor-menu"
        const val SETTINGS = "app-editor-settings"
        const val VERSION_PREVIEW = "app-version-preview"
        const val VERSION_RESTORE = "app-version-restore"
        // eApps V1: the editor's Publish, a Versions row's Publish.
        const val PUBLISH = "app-editor-publish"
        const val VERSION_PUBLISH = "app-version-publish"
        // VAPP-178: the Data tab's body.
        const val DATA = "app-editor-data"
        // VAPP-179: the Preview toolbar (web ≥ md + desktop only; phones
        // have none, the ids stay in step with the fixture).
        const val PREVIEW_TOOLBAR = "app-preview-toolbar"
        const val PREVIEW_FRAME = "app-preview-frame"
        const val PREVIEW_TEMPLATE = "app-preview-template"
        // VAPP-198: Run as (in the toolbar: web ≥ md + desktop only).
        const val PREVIEW_RUN_AS = "app-preview-run-as"
        // #VAPP-220: the Settings tab's Automations group + one of its rows.
        const val AUTOMATIONS = "app-settings-automations"
        const val AUTOMATION_ROW = "app-automation-row"
        // #VAPP-238: the Webhooks group, one of its rows, the shown-once secret.
        const val WEBHOOKS = "app-settings-webhooks"
        const val WEBHOOK_ROW = "app-webhook-row"
        const val WEBHOOK_SECRET = "app-webhook-secret"
        // VAPP-218: the preview frame's screens nav (sidebar or bar).
        const val PREVIEW_SCREENS = "app-editor-preview-screens"
        // VAPP-242: THE RUN VIEW, its `…` menu, THE NEW MENU.
        const val RUN = "app-run"
        const val RUN_MENU = "app-run-menu"
        const val NEW_MENU = "apps-new-menu"
        // #VAPP-243: the Accounts and MCP tools groups + one of their rows.
        const val ACCOUNTS = "app-settings-accounts"
        const val ACCOUNT_ROW = "app-account"
        const val IMPORT_UPSTREAMS = "app-import-upstreams"
        const val IMPORT_UPSTREAM = "app-import-upstream"
        const val MCP_TOOLS = "app-settings-mcp-tools"
        const val MCP_TOOL_ROW = "app-mcp-tool"
        // #VAPP-243: the Record access group + one of its rows.
        const val POLICIES = "app-settings-policies"
        const val POLICY_ROW = "app-policy"
    }

    /** VAPP-177 (fixture `editorTabs`): the editor's tabs. Wide (web ≥ md,
     *  desktop) = Preview | Data | Versions | Settings; phones = Chat |
     *  Preview | Data + the `…` menu's Versions / Settings sheets (VAPP-178). */
    object EditorTabs {
        const val CHAT = "chat"
        const val PREVIEW = "preview"
        const val DATA = "data"
        const val VERSIONS = "versions"
        const val SETTINGS = "settings"
        val WIDE = listOf(PREVIEW, DATA, VERSIONS, SETTINGS)
        val PHONE = listOf(CHAT, PREVIEW, DATA)
        val MENU = listOf("versions", "settings", "export", "delete")
        const val DEFAULT = PREVIEW
        const val PHONE_DEFAULT = CHAT
        const val SETTINGS_MAX_WIDTH = 640
        const val SEARCH = "tab"
    }

    /** The tab on screen and, on phones, the sheet over it. */
    data class EditorTabPick(val tab: String, val sheet: String?)

    /** A requested tab (web `?tab=`, the desktop settings nav) → the tab on
     *  screen and, on phones, the sheet over it (`editorTabCases`). */
    fun editorTab(requested: String?, phone: Boolean): EditorTabPick {
        if (phone) {
            if (requested == EditorTabs.VERSIONS || requested == EditorTabs.SETTINGS) {
                return EditorTabPick(EditorTabs.PHONE_DEFAULT, requested)
            }
            return EditorTabPick(if (requested in EditorTabs.PHONE) requested!! else EditorTabs.PHONE_DEFAULT, null)
        }
        return EditorTabPick(if (requested in EditorTabs.WIDE) requested!! else EditorTabs.DEFAULT, null)
    }

    /** VAPP-176: a DRAFT = an app with no version yet (head_version 0). */
    fun isDraft(headVersion: Int): Boolean = headVersion == 0

    /** #FEED-204 (`editorDeviceCases`, apps.json EDITOR DEVICE): the device an
     *  app editor's composer shows and sends on ([deviceId] null = none online
     *  and no root to fall back on), [move] when an ended root's next send goes
     *  to another device. A live root keeps its device: only a pick moves it. */
    data class EditorDeviceChoice(val deviceId: String?, val move: Boolean)

    data class EditorDeviceRoot(val status: String, val deviceId: String?)

    data class EditorDeviceCandidate(val deviceId: String, val isDefault: Boolean)

    /** [candidates] = the member's online devices (with a root: the targets a
     *  move accepts). */
    fun editorDevice(
        root: EditorDeviceRoot?,
        candidates: List<EditorDeviceCandidate>,
        remembered: String?,
    ): EditorDeviceChoice {
        if (root != null && root.status != DomainContract.sessionStatusEnded) {
            return EditorDeviceChoice(root.deviceId, move = false)
        }
        fun known(id: String?) = id != null && candidates.any { it.deviceId == id }
        val deviceId = when {
            known(remembered) -> remembered
            known(root?.deviceId) -> root?.deviceId
            else -> candidates.firstOrNull { it.isDefault }?.deviceId ?: candidates.firstOrNull()?.deviceId ?: root?.deviceId
        }
        return EditorDeviceChoice(deviceId, move = root != null && deviceId != root.deviceId)
    }

    // Contract `apps` (packages/domain-contract/contract.json), locked by AppsTest.
    const val SLUG_MAX = 64
    const val NAME_MAX = 64
    const val DESCRIPTION_MAX = 2000
    const val NOTE_MAX = 500
    // VAPP-122 records: a kind's name, one record's `data` JSON, kinds per manifest.
    const val KIND_PATTERN = "^[a-z][a-z0-9_]{0,31}$"
    const val RECORD_MAX_BYTES = 16384
    const val RECORD_KINDS_MAX = 16

    /** `v{n}`, or the "no version" copy while an app has no head. */
    fun versionLabel(headVersion: Int): String =
        if (headVersion > 0) Copy.VERSION.replace("{n}", headVersion.toString()) else Copy.NO_VERSION

    fun viewing(number: Int): String = Copy.VIEWING.replace("{n}", number.toString())

    fun publishedToast(number: Int): String = Copy.PUBLISHED_TOAST.replace("{n}", number.toString())

    // eApps V1 draft vs published (#VAPP-210, fixture `publishCases`).

    /** An app's two pointers. */
    data class Versions(val headVersion: Int, val publishedVersion: Int)

    /** A Versions row's trailing (TS `versionRow`). */
    enum class VersionRowKind(val wire: String) { PUBLISHED("published"), HEAD("head"), VERSION("version") }

    /** The version the editor's preview shows: an explicitly viewed one, else
     *  nothing (no version), else the HEAD (the draft) while the viewer's own
     *  editor session is live or nothing is published, else the PUBLISHED one. */
    fun shownVersion(app: Versions, editing: Boolean, viewing: Int?): Int {
        if (viewing != null) return viewing
        if (app.headVersion == 0) return 0
        return if (editing || app.publishedVersion == 0) app.headVersion else app.publishedVersion
    }

    /** Publish is offered while the head is not the published version. */
    fun canPublish(app: Versions): Boolean = app.headVersion > 0 && app.headVersion != app.publishedVersion

    /** The list's and the header's version pill: the PUBLISHED version. */
    fun listPill(app: Versions): String = versionLabel(app.publishedVersion)

    /** The version a grant is made on and checked against (`apps.update
     *  {headVersion}` carries it): the published one, else the head. */
    fun grantVersion(app: Versions): Int = if (app.publishedVersion > 0) app.publishedVersion else app.headVersion

    /** A Versions row's trailing: the published badge, the head badge +
     *  Publish, or Preview + Publish. */
    /** eApps R8 (#VAPP-233, fixture `migrationCases`): a version row's sweep line after its subtitle, or null. */
    fun versionMigration(state: String?, total: Int, done: Int, error: String?): String? = when (state) {
        "running" -> if (total > 0) Copy.MIGRATING_OF.replace("{done}", done.toString()).replace("{total}", total.toString()) else Copy.MIGRATING
        "failed" -> Copy.MIGRATION_FAILED.replace("{error}", error ?: "")
        else -> null
    }

    /** #VAPP-256 (fixture `rollbackCases`): whether publishing [target] rolls
     *  back past a version whose sweep ran, so the Publish asks first.
     *  [rows] = (number, migrationState). */
    fun rollbackWarns(published: Int, target: Int, rows: List<Pair<Int, String?>>): Boolean =
        target < published && rows.any { (number, state) -> number > target && state != null }

    fun rollbackTitle(n: Int): String = Copy.ROLLBACK_TITLE.replace("{n}", n.toString())
    fun rollbackBody(n: Int): String = Copy.ROLLBACK_BODY.replace("{n}", n.toString())

    fun versionRow(app: Versions, number: Int): VersionRowKind = when (number) {
        app.publishedVersion -> VersionRowKind.PUBLISHED
        app.headVersion -> VersionRowKind.HEAD
        else -> VersionRowKind.VERSION
    }

    // #VAPP-220 (fixture `automationRowCases`): one row of the Settings tab's
    // Automations group, from tRPC `appAccess.appTools` `automations`.

    /** An automation's trigger (`on`). */
    sealed interface AutomationTrigger {
        data class Schedule(val cron: String, val tz: String? = null) : AutomationTrigger
        data class Record(val kind: String, val op: String) : AutomationTrigger
        data class Event(val event: String) : AutomationTrigger
        /** #VAPP-237: a webhook delivery (the hook name). */
        data class Webhook(val name: String) : AutomationTrigger
    }

    data class AutomationRowInput(
        val name: String,
        val on: AutomationTrigger,
        val paused: String?,
        val lastStatus: String?,
    )

    enum class AutomationStatus(val wire: String) { PAUSED("paused"), FAILED("failed"), OK("ok"), NEVER("never") }

    data class AutomationRow(val trigger: String, val status: AutomationStatus, val label: String)

    /** trigger = the filled `copy.automations` line; status = paused when the
     *  row names a pause, else never without a last run, else failed when the
     *  last run is not `ok`, else ok; label = the status's copy. */
    fun automationRow(row: AutomationRowInput): AutomationRow {
        val trigger = when (val on = row.on) {
            is AutomationTrigger.Schedule ->
                if (!on.tz.isNullOrEmpty()) {
                    AutomationsCopy.SCHEDULE_ZONE.replace("{cron}", on.cron).replace("{tz}", on.tz)
                } else {
                    AutomationsCopy.SCHEDULE.replace("{cron}", on.cron)
                }
            is AutomationTrigger.Record -> when (on.op) {
                "create" -> AutomationsCopy.CREATE
                "update" -> AutomationsCopy.UPDATE
                else -> AutomationsCopy.DELETE
            }.replace("{kind}", on.kind)
            is AutomationTrigger.Event -> AutomationsCopy.EVENT.replace("{event}", on.event)
            is AutomationTrigger.Webhook -> AutomationsCopy.WEBHOOK.replace("{name}", on.name)
        }
        val status = when {
            !row.paused.isNullOrEmpty() -> AutomationStatus.PAUSED
            row.lastStatus == null -> AutomationStatus.NEVER
            row.lastStatus != "ok" -> AutomationStatus.FAILED
            else -> AutomationStatus.OK
        }
        val label = when (status) {
            AutomationStatus.PAUSED -> AutomationsCopy.PAUSED
            AutomationStatus.FAILED -> AutomationsCopy.FAILED
            AutomationStatus.OK -> AutomationsCopy.OK
            AutomationStatus.NEVER -> AutomationsCopy.NEVER
        }
        return AutomationRow(trigger, status, label)
    }

    /** #VAPP-243 (`automationSponsorCases`): who an automation runs as, or
     *  why it is paused (a pause wins); null = the trigger line alone. */
    fun automationSponsor(paused: String?, sponsorName: String?): String? {
        if (!paused.isNullOrEmpty()) return AutomationsCopy.PAUSED_REASONS[paused] ?: AutomationsCopy.PAUSED
        return sponsorName?.takeIf { it.isNotEmpty() }?.let { AutomationsCopy.RUNS_AS.replace("{name}", it) }
    }

    /** #VAPP-255 (`automationDroppedCases`): the runs the rate cap dropped in
     *  the last 24 h; null = nothing to say. */
    fun automationDropped(n: Int): String? = when {
        n <= 0 -> null
        n == 1 -> AutomationsCopy.DROPPED_ONE
        else -> AutomationsCopy.DROPPED.replace("{n}", n.toString())
    }

    // #VAPP-238 (fixture `webhookRowCases`): one row of the Settings tab's
    // Webhooks group, from tRPC `appAccess.appTools` `hooks`; isOwner = `canToggle`.

    data class WebhookRowInput(val name: String, val hasSecret: Boolean)

    enum class WebhookStatus(val wire: String) { READY("ready"), UNSIGNED("unsigned") }

    enum class WebhookAction(val wire: String) { MINT("mint"), ROTATE("rotate") }

    data class WebhookRow(
        val status: WebhookStatus,
        val label: String,
        val action: WebhookAction?,
        val actionLabel: String?,
    )

    /** status = ready with a secret, else unsigned; an owner gets mint (no
     *  secret yet) or rotate, everyone else no action. */
    fun webhookRow(row: WebhookRowInput, isOwner: Boolean): WebhookRow {
        val status = if (row.hasSecret) WebhookStatus.READY else WebhookStatus.UNSIGNED
        val action = if (!isOwner) null else if (row.hasSecret) WebhookAction.ROTATE else WebhookAction.MINT
        return WebhookRow(
            status,
            if (status == WebhookStatus.READY) WebhooksCopy.READY else WebhooksCopy.UNSIGNED,
            action,
            when (action) {
                WebhookAction.MINT -> WebhooksCopy.MINT
                WebhookAction.ROTATE -> WebhooksCopy.ROTATE
                null -> null
            },
        )
    }

    // #VAPP-243 (fixture `accountRowCases`): one row of the Settings tab's
    // Accounts group, from tRPC `appAccess.appTools` `upstreams`.

    data class AccountRowInput(val name: String, val serverId: String?, val state: String?)

    enum class AccountAction(val wire: String) { CONNECT("connect"), RECONNECT("reconnect") }

    /** [status] = `notSetUp` or the upstream.json state; [label] = the muted
     *  trailing text when there is no action. */
    data class AccountRow(val status: String, val label: String?, val action: AccountAction?, val actionLabel: String?)

    // #VAPP-254 (fixture `upstreamPickerCases`): an Accounts or import row's
    // Select, and the second line naming the real server.
    const val UPSTREAM_PICKER_NONE = "none"

    data class UpstreamPickerOption(val value: String, val label: String)

    data class UpstreamPicker(val value: String, val options: List<UpstreamPickerOption>)

    /** `servers` = (id, name) in name order; empty = no Select. */
    fun upstreamPicker(name: String, serverId: String?, servers: List<Pair<String, String>>): UpstreamPicker? {
        if (servers.isEmpty()) return null
        val clear = if (servers.any { it.second == name }) emptyList() else listOf(UpstreamPickerOption(UPSTREAM_PICKER_NONE, AccountsCopy.NOT_SET_UP))
        return UpstreamPicker(serverId ?: UPSTREAM_PICKER_NONE, clear + servers.map { UpstreamPickerOption(it.first, it.second) })
    }

    fun accountCaption(name: String, serverName: String?): String? =
        serverName?.takeIf { it != name }?.let { AccountsCopy.USES_SERVER.replace("{server}", it) }

    fun accountRow(row: AccountRowInput): AccountRow {
        if (row.serverId == null) return AccountRow("notSetUp", AccountsCopy.NOT_SET_UP, null, null)
        val status = row.state ?: "disconnected"
        if (status == "connected" || status == "expired") return AccountRow(status, AccountsCopy.CONNECTED, null, null)
        val action = if (status == "reconnect") AccountAction.RECONNECT else AccountAction.CONNECT
        return AccountRow(status, null, action, if (action == AccountAction.RECONNECT) AccountsCopy.RECONNECT else AccountsCopy.CONNECT)
    }

    // #VAPP-243 (fixture `policyRowCases`): one Record access row = the
    // kind and its EFFECTIVE policy (AppRecords.Policy.effective) as text.

    data class PolicyRow(val kind: String, val caption: String)

    fun policyCaption(policy: Map<String, String>): String =
        AppRecords.Policy.OPS.joinToString(" · ") { op ->
            val level = policy[op] ?: AppRecords.Policy.DEFAULTS[op] ?: "member"
            "${PoliciesCopy.OPS[op] ?: op}: ${PoliciesCopy.LEVELS[level] ?: level}"
        }

    /** The package's declared kinds in code-point order with their captions. */
    fun policyRows(pkg: at.exponential.ui.json.JsonValue?): List<PolicyRow> {
        val records = pkg?.obj?.get("records")?.obj ?: return emptyList()
        return records.keys.sorted().map { PolicyRow(it, policyCaption(AppRecords.Policy.effective(records[it]))) }
    }

    fun restored(restored: Int, version: Int): String =
        Copy.RESTORED.replace("{n}", restored.toString()).replace("{m}", version.toString())

    fun deleteTitle(name: String): String = Copy.DELETE_TITLE.replace("{name}", name)

    private val COMBINING = Regex("[\\u0300-\\u036f]")
    private val NON_SLUG = Regex("[^a-z0-9]+")
    private val EDGE_DASHES = Regex("^-+|-+$")
    private val TRAILING_DASHES = Regex("-+$")

    /** A name → its slug: lowercase ASCII letters, digits and single dashes,
     *  diacritics stripped, cut at the cap, `app` when nothing is left; a taken
     *  slug gets `-2`, `-3`, … */
    fun slugify(name: String, taken: Collection<String> = emptyList()): String {
        val base = Normalizer.normalize(name, Normalizer.Form.NFKD)
            .replace(COMBINING, "")
            .lowercase()
            .replace(NON_SLUG, "-")
            .replace(EDGE_DASHES, "")
            .take(SLUG_MAX)
            .replace(TRAILING_DASHES, "")
            .ifEmpty { "app" }
        if (base !in taken) return base
        var n = 2
        while (true) {
            val suffix = "-$n"
            val candidate = base.take(SLUG_MAX - suffix.length).replace(TRAILING_DASHES, "") + suffix
            if (candidate !in taken) return candidate
            n++
        }
    }

    /** What [sortApps] orders by. */
    interface ListLike {
        val slug: String
        val name: String
    }

    /** The list order: by name (case-insensitive). Exponential ships no built-in apps. */
    fun <T : ListLike> sortApps(apps: List<T>): List<T> =
        apps.sortedWith(compareBy(String.CASE_INSENSITIVE_ORDER) { it.name })

    /** VAPP-179: the preview's template ids, sorted by id (web `previewTemplates`;
     *  a JSON layer, Postgres JSONB included, does not keep the declared order). */
    fun previewTemplates(templateIds: Collection<String>): List<String> = templateIds.sorted()

    /** VAPP-179 the shown template (fixture `templateCases`): [picked] when the
     *  package has it, else the first of [previewTemplates]; null = no template.
     *  Phones have no picker: they pass null. */
    fun previewTemplate(templateIds: Collection<String>, picked: String? = null): String? {
        val list = previewTemplates(templateIds)
        return if (picked != null && picked in list) picked else list.firstOrNull()
    }

    /** The fixture's `mediaPolicy` (`AppsTest` locks the copy). */
    object MediaPolicy {
        val DATA_MIME_PREFIXES = listOf("image/", "audio/", "video/")
        val DENIED_DATA_MIMES = listOf("image/svg+xml")
        const val ATTACHMENT_PATH = "/api/attachments/"
    }

    private val ATTACHMENT_PATH_RE = Regex(
        "${Regex.escape(MediaPolicy.ATTACHMENT_PATH)}[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}",
        RegexOption.IGNORE_CASE,
    )

    /** VAPP-157: the app hosts' media rule (fixture `mediaPolicy`), a plain
     *  string rule so the four hosts agree byte for byte: a `data:` image,
     *  audio or video (never svg), or the instance's own
     *  `/api/attachments/<uuid>` (relative or on [origin] exactly) with
     *  nothing after the id. */
    fun appMediaAllowed(src: String, origin: String): Boolean {
        if (src.isEmpty() || src.trim() != src) return false
        val lower = src.lowercase()
        if (lower.startsWith("data:")) {
            val comma = lower.indexOf(',')
            if (comma < 0) return false
            val mime = lower.substring(5, comma).split(';')[0]
            return MediaPolicy.DATA_MIME_PREFIXES.any { mime.startsWith(it) && mime.length > it.length } &&
                mime !in MediaPolicy.DENIED_DATA_MIMES
        }
        val path = if (src.startsWith("/") && !src.startsWith("//")) {
            src
        } else {
            val base = origin.trimEnd('/').lowercase()
            if (base.isEmpty() || !lower.startsWith("$base/")) return false
            src.substring(base.length)
        }
        return ATTACHMENT_PATH_RE.matches(path)
    }

    /** VAPP-221 (eApps R7): the app hosts' FileUpload uploads (fixture
     *  `appFiles`, `AppsTest` locks it): every picked file POSTs to
     *  [UPLOAD_PATH] as one part [PART_NAME]; the answer's `url` lands in the
     *  FileUpload's bound list. */
    object AppFiles {
        const val UPLOAD_PATH = "/api/apps/{appId}/files"
        const val PART_NAME = "file"
        const val FAILED_VERB = "upload"
        const val FAILED_THING = "the file"
    }

    /** The upload path of an app ([AppFiles.UPLOAD_PATH] with its id). */
    fun appUploadPath(appId: String): String =
        AppFiles.UPLOAD_PATH.replace("{appId}", URLEncoder.encode(appId, "UTF-8").replace("+", "%20"))

    /** The absolute `files` path of the FIRST FileUpload (component order)
     *  named [name] (its `name`, else its id); null when it has no absolute
     *  `{"@path"}` binding (`targetCases`). */
    fun appUploadTarget(components: List<JsonValue>, name: String): String? {
        val upload = components.firstOrNull { c ->
            c["component"]?.string == "FileUpload" && (c["name"]?.let { it as? JsonValue.Str }?.v ?: c["id"]?.string) == name
        } ?: return null
        val files = (upload["files"] as? JsonValue.Obj)?.v ?: return null
        val path = files["@path"]?.string ?: return null
        return path.takeIf { it.startsWith("/") }
    }

    /** [list] with [url] on every object entry named [fileName]; null =
     *  nothing to write (no list, no such entry) (`urlCases`). */
    fun withUploadedUrl(list: JsonValue?, fileName: String, url: String): JsonValue? {
        val items = (list as? JsonValue.Arr)?.v ?: return null
        var hit = false
        val next = items.map { entry ->
            if (entry !is JsonValue.Obj || entry.v["name"]?.string != fileName) return@map entry
            hit = true
            JsonValue.Obj(LinkedHashMap(entry.v).apply { put("url", JsonValue.Str(url)) })
        }
        return if (hit) JsonValue.Arr(next) else null
    }

    /** VAPP-218 (eApps U1, fixture `screens`): a package's screens, painted by
     *  the host inside the preview frame with the existing navigation only. */
    object Screens {
        const val SIDEBAR_WIDTH = 200
        val RESERVED = listOf("tab", "frame")
    }

    enum class ScreenShell(val wire: String) { Sidebar("sidebar"), Bar("bar") }

    /** #VAPP-251 (`surfaceStateCases`): what an app's pane shows. */
    enum class SurfaceState { Empty, Surface, NeedsUpdate }

    /** Nothing to run, the surface, or once the host had its chance and holds
     *  no surface (it refused the package), [Copy.NEEDS_UPDATE]. */
    fun surfaceState(hasPackage: Boolean, applied: Boolean, hasSurface: Boolean): SurfaceState = when {
        !hasPackage -> SurfaceState.Empty
        applied && !hasSurface -> SurfaceState.NeedsUpdate
        else -> SurfaceState.Surface
    }

    /** Which navigation the preview frame paints (`screenShellCases`): none
     *  without screens, the phone bottom bar on a phone, else the sidebar. */
    fun screenShell(screenCount: Int, phone: Boolean): ScreenShell? = when {
        screenCount <= 0 -> null
        phone -> ScreenShell.Bar
        else -> ScreenShell.Sidebar
    }

    /** The web deep link to an app screen (`screenPathCases`):
     *  `/t/<team>/apps/<app>/<screen>`, then the params sorted by key, each
     *  part `encodeURIComponent`-encoded. */
    fun screenPath(teamSlug: String, appSlug: String, screen: String, params: Map<String, String> = emptyMap()): String {
        val path = "/t/${encodeUriComponent(teamSlug)}/apps/${encodeUriComponent(appSlug)}/${encodeUriComponent(screen)}"
        if (params.isEmpty()) return path
        return path + "?" + params.keys.sorted().joinToString("&") { "${encodeUriComponent(it)}=${encodeUriComponent(params.getValue(it))}" }
    }

    /** A screen route's query → its params: every key but [Screens.RESERVED],
     *  sorted (`screenQueryCases`). */
    fun screenParamsFromQuery(query: Map<String, String>): Map<String, String> =
        query.keys.sorted().filter { it !in Screens.RESERVED }.associateWith { query.getValue(it) }

    /** JS `encodeURIComponent`: UTF-8 bytes, all but `A-Z a-z 0-9 - _ . ! ~ * ' ( )` as `%XX`. */
    private fun encodeUriComponent(value: String): String = buildString {
        for (byte in value.toByteArray(Charsets.UTF_8)) {
            val c = (byte.toInt() and 0xff).toChar()
            if (c in 'a'..'z' || c in 'A'..'Z' || c in '0'..'9' || c in "-_.!~*'()") append(c)
            else append('%').append("%02X".format(byte.toInt() and 0xff))
        }
    }
}
