package com.exponential.app.domain

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test

// VAPP-110: replays `packages/domain-contract/fixtures/apps.json` (web,
// desktop and iOS replay the same cases) and locks the copy + caps.
class AppsTest {
    private fun read(relative: String): JsonObject {
        val candidates = listOf("../../../$relative", "../../$relative", relative)
        val file = candidates.map(::File).firstOrNull { it.isFile } ?: error("$relative not found from ${File(".").absolutePath}")
        return Json.parseToJsonElement(file.readText()).jsonObject
    }

    private val fixture = read("packages/domain-contract/fixtures/apps.json")
    private val contract = read("packages/domain-contract/contract.json")

    private data class Row(override val slug: String, override val name: String) : Apps.ListLike

    @Test
    fun `slugify replays the fixture`() {
        for (case in fixture["slugifyCases"]!!.jsonArray.map { it.jsonObject }) {
            val taken = case["taken"]?.jsonArray?.map { it.jsonPrimitive.content }.orEmpty()
            assertEquals(case.toString(), case["slug"]!!.jsonPrimitive.content, Apps.slugify(case["name"]!!.jsonPrimitive.content, taken))
        }
    }

    @Test
    fun `versionLabel replays the fixture`() {
        for (case in fixture["versionLabelCases"]!!.jsonArray.map { it.jsonObject }) {
            assertEquals(case["label"]!!.jsonPrimitive.content, Apps.versionLabel(case["headVersion"]!!.jsonPrimitive.int))
        }
    }

    // eApps R8 (#VAPP-233): versionMigration replays `migrationCases`.
    @Test
    fun `migrationCases replay`() {
        val cases = fixture["migrationCases"]!!.jsonObject["cases"]!!.jsonArray.map { it.jsonObject }
        assert(cases.isNotEmpty())
        for (case in cases) {
            val row = case["row"]!!.jsonObject
            val str = { k: String -> row[k]!!.let { if (it is JsonNull) null else it.jsonPrimitive.content } }
            val got = Apps.versionMigration(str("migrationState"), row["migrationTotal"]!!.jsonPrimitive.int, row["migrationDone"]!!.jsonPrimitive.int, str("migrationError"))
            val want = case["expected"]!!.let { if (it is JsonNull) null else it.jsonPrimitive.content }
            assertEquals(case["name"]!!.jsonPrimitive.content, want, got)
        }
    }

    // eApps V1 (#VAPP-210): shownVersion, canPublish, grantVersion, listPill
    // and versionRow replay `publishCases`.
    @Test
    fun `publishCases replay`() {
        val cases = fixture["publishCases"]!!.jsonObject["cases"]!!.jsonArray.map { it.jsonObject }
        assert(cases.isNotEmpty())
        for (case in cases) {
            val name = case["name"]!!.jsonPrimitive.content
            val raw = case["app"]!!.jsonObject
            val app = Apps.Versions(raw["headVersion"]!!.jsonPrimitive.int, raw["publishedVersion"]!!.jsonPrimitive.int)
            val viewing = case["viewing"]!!.let { if (it is JsonNull) null else it.jsonPrimitive.int }
            val editing = case["editing"]!!.jsonPrimitive.content.toBooleanStrict()
            assertEquals(name, case["shown"]!!.jsonPrimitive.int, Apps.shownVersion(app, editing, viewing))
            assertEquals(name, case["canPublish"]!!.jsonPrimitive.content.toBooleanStrict(), Apps.canPublish(app))
            assertEquals(name, case["grant"]!!.jsonPrimitive.int, Apps.grantVersion(app))
            assertEquals(name, case["pill"]!!.jsonPrimitive.content, Apps.listPill(app))
            for (row in case["rows"]!!.jsonArray.map { it.jsonArray }) {
                val number = row[0].jsonPrimitive.int
                assertEquals("$name v$number", row[1].jsonPrimitive.content, Apps.versionRow(app, number).wire)
            }
        }
    }

    // #VAPP-220: automationRow replays `automationRowCases` and locks `copy.automations`.
    @Test
    fun `automationRowCases replay`() {
        val copy = fixture["copy"]!!.jsonObject["automations"]!!.jsonObject
        val expectedCopy = mapOf(
            "title" to Apps.AutomationsCopy.TITLE, "schedule" to Apps.AutomationsCopy.SCHEDULE,
            "scheduleZone" to Apps.AutomationsCopy.SCHEDULE_ZONE, "create" to Apps.AutomationsCopy.CREATE,
            "update" to Apps.AutomationsCopy.UPDATE, "delete" to Apps.AutomationsCopy.DELETE,
            "event" to Apps.AutomationsCopy.EVENT, "webhook" to Apps.AutomationsCopy.WEBHOOK, "paused" to Apps.AutomationsCopy.PAUSED,
            "failed" to Apps.AutomationsCopy.FAILED, "ok" to Apps.AutomationsCopy.OK, "never" to Apps.AutomationsCopy.NEVER,
            "runsAs" to Apps.AutomationsCopy.RUNS_AS,
        )
        assertEquals(copy.keys - "pausedReasons", expectedCopy.keys)
        assertEquals(
            copy["pausedReasons"]!!.jsonObject.mapValues { it.value.jsonPrimitive.content },
            Apps.AutomationsCopy.PAUSED_REASONS,
        )
        for ((key, value) in expectedCopy) assertEquals(key, copy[key]!!.jsonPrimitive.content, value)

        val cases = fixture["automationRowCases"]!!.jsonObject["cases"]!!.jsonArray.map { it.jsonObject }
        assert(cases.isNotEmpty())
        fun opt(obj: JsonObject, key: String) = obj[key]?.let { if (it is JsonNull) null else it.jsonPrimitive.content }
        for (case in cases) {
            val name = case["name"]!!.jsonPrimitive.content
            val row = case["row"]!!.jsonObject
            val on = row["on"]!!.jsonObject
            val trigger = when {
                "schedule" in on -> on["schedule"]!!.jsonObject.let {
                    Apps.AutomationTrigger.Schedule(it["cron"]!!.jsonPrimitive.content, opt(it, "tz"))
                }
                "record" in on -> on["record"]!!.jsonObject.let {
                    Apps.AutomationTrigger.Record(it["kind"]!!.jsonPrimitive.content, it["op"]!!.jsonPrimitive.content)
                }
                "webhook" in on -> Apps.AutomationTrigger.Webhook(on["webhook"]!!.jsonObject["name"]!!.jsonPrimitive.content)
                else -> Apps.AutomationTrigger.Event(on["event"]!!.jsonPrimitive.content)
            }
            val got = Apps.automationRow(
                Apps.AutomationRowInput(row["name"]!!.jsonPrimitive.content, trigger, opt(row, "paused"), opt(row, "lastStatus")),
            )
            val expected = case["expected"]!!.jsonObject
            assertEquals(name, expected["trigger"]!!.jsonPrimitive.content, got.trigger)
            assertEquals(name, expected["status"]!!.jsonPrimitive.content, got.status.wire)
            assertEquals(name, expected["label"]!!.jsonPrimitive.content, got.label)
        }
    }

    // #VAPP-243: automationSponsor replays `automationSponsorCases`.
    @Test
    fun `automationSponsorCases replay`() {
        val cases = fixture["automationSponsorCases"]!!.jsonObject["cases"]!!.jsonArray.map { it.jsonObject }
        assert(cases.isNotEmpty())
        fun opt(obj: JsonObject, key: String) = obj[key]?.let { if (it is JsonNull) null else it.jsonPrimitive.content }
        for (case in cases) {
            assertEquals(
                case["name"]!!.jsonPrimitive.content,
                opt(case, "expected"),
                Apps.automationSponsor(opt(case, "paused"), opt(case, "sponsorName")),
            )
        }
    }

    // #VAPP-238: webhookRow replays `webhookRowCases` and locks `copy.webhooks`.
    @Test
    fun `webhookRowCases replay`() {
        val copy = fixture["copy"]!!.jsonObject["webhooks"]!!.jsonObject
        val expectedCopy = mapOf(
            "title" to Apps.WebhooksCopy.TITLE, "ready" to Apps.WebhooksCopy.READY,
            "unsigned" to Apps.WebhooksCopy.UNSIGNED, "copyUrl" to Apps.WebhooksCopy.COPY_URL,
            "copied" to Apps.WebhooksCopy.COPIED, "mint" to Apps.WebhooksCopy.MINT,
            "rotate" to Apps.WebhooksCopy.ROTATE, "secret" to Apps.WebhooksCopy.SECRET,
            "copySecret" to Apps.WebhooksCopy.COPY_SECRET,
        )
        assertEquals(copy.keys, expectedCopy.keys)
        for ((key, value) in expectedCopy) assertEquals(key, copy[key]!!.jsonPrimitive.content, value)

        val cases = fixture["webhookRowCases"]!!.jsonObject["cases"]!!.jsonArray.map { it.jsonObject }
        assert(cases.isNotEmpty())
        fun opt(obj: JsonObject, key: String) = obj[key]?.let { if (it is JsonNull) null else it.jsonPrimitive.content }
        for (case in cases) {
            val name = case["name"]!!.jsonPrimitive.content
            val row = case["row"]!!.jsonObject
            val got = Apps.webhookRow(
                Apps.WebhookRowInput(row["name"]!!.jsonPrimitive.content, row["hasSecret"]!!.jsonPrimitive.boolean),
                case["isOwner"]!!.jsonPrimitive.boolean,
            )
            val expected = case["expected"]!!.jsonObject
            assertEquals(name, expected["status"]!!.jsonPrimitive.content, got.status.wire)
            assertEquals(name, expected["label"]!!.jsonPrimitive.content, got.label)
            assertEquals(name, opt(expected, "action"), got.action?.wire)
            assertEquals(name, opt(expected, "actionLabel"), got.actionLabel)
        }
    }

    // #VAPP-243: accountRow replays `accountRowCases` and locks `copy.accounts` + `copy.mcpTools`.
    @Test
    fun `accountRowCases replay`() {
        val accounts = fixture["copy"]!!.jsonObject["accounts"]!!.jsonObject
        val expectedAccounts = mapOf(
            "title" to Apps.AccountsCopy.TITLE, "connect" to Apps.AccountsCopy.CONNECT,
            "reconnect" to Apps.AccountsCopy.RECONNECT, "connected" to Apps.AccountsCopy.CONNECTED,
            "notSetUp" to Apps.AccountsCopy.NOT_SET_UP,
        )
        assertEquals(accounts.keys, expectedAccounts.keys)
        for ((key, value) in expectedAccounts) assertEquals(key, accounts[key]!!.jsonPrimitive.content, value)
        val tools = fixture["copy"]!!.jsonObject["mcpTools"]!!.jsonObject
        val expectedTools = mapOf("title" to Apps.McpToolsCopy.TITLE, "read" to Apps.McpToolsCopy.READ, "write" to Apps.McpToolsCopy.WRITE)
        assertEquals(tools.keys, expectedTools.keys)
        for ((key, value) in expectedTools) assertEquals(key, tools[key]!!.jsonPrimitive.content, value)

        val cases = fixture["accountRowCases"]!!.jsonObject["cases"]!!.jsonArray.map { it.jsonObject }
        assert(cases.isNotEmpty())
        fun opt(obj: JsonObject, key: String) = obj[key]?.let { if (it is JsonNull) null else it.jsonPrimitive.content }
        for (case in cases) {
            val name = case["name"]!!.jsonPrimitive.content
            val row = case["row"]!!.jsonObject
            val got = Apps.accountRow(Apps.AccountRowInput(row["name"]!!.jsonPrimitive.content, opt(row, "serverId"), opt(row, "state")))
            val expected = case["expected"]!!.jsonObject
            assertEquals(name, expected["status"]!!.jsonPrimitive.content, got.status)
            assertEquals(name, opt(expected, "label"), got.label)
            assertEquals(name, opt(expected, "action"), got.action?.wire)
            assertEquals(name, opt(expected, "actionLabel"), got.actionLabel)
        }
    }

    // #VAPP-243: policyCaption replays `policyRowCases` and locks `copy.policies`.
    @Test
    fun `policyRowCases replay`() {
        val copy = fixture["copy"]!!.jsonObject["policies"]!!.jsonObject
        assertEquals(copy["title"]!!.jsonPrimitive.content, Apps.PoliciesCopy.TITLE)
        assertEquals(copy["ops"]!!.jsonObject.mapValues { it.value.jsonPrimitive.content }, Apps.PoliciesCopy.OPS)
        assertEquals(copy["levels"]!!.jsonObject.mapValues { it.value.jsonPrimitive.content }, Apps.PoliciesCopy.LEVELS)
        val cases = fixture["policyRowCases"]!!.jsonObject["cases"]!!.jsonArray.map { it.jsonObject }
        assert(cases.isNotEmpty())
        for (case in cases) {
            val policy = case["policy"]!!.jsonObject.mapValues { it.value.jsonPrimitive.content }
            assertEquals(case["name"]!!.jsonPrimitive.content, case["expected"]!!.jsonPrimitive.content, Apps.policyCaption(policy))
        }
        val rows = Apps.policyRows(at.exponential.ui.json.JsonValue.parse("""{"records":{"note":{},"entry":{"policy":{"read":"author"}}}}"""))
        assertEquals(listOf("entry", "note"), rows.map { it.kind })
        assertEquals("Read: Author · Add: Members · Change: Author · Delete: Author", rows.first().caption)
    }

    @Test
    fun `editorTab replays the fixture`() {
        val tabs = fixture["editorTabs"]!!.jsonObject
        fun list(key: String) = tabs[key]!!.jsonArray.map { it.jsonPrimitive.content }
        assertEquals(list("wide"), Apps.EditorTabs.WIDE)
        assertEquals(list("phone"), Apps.EditorTabs.PHONE)
        assertEquals(list("menu"), Apps.EditorTabs.MENU)
        assertEquals(tabs["default"]!!.jsonPrimitive.content, Apps.EditorTabs.DEFAULT)
        assertEquals(tabs["phoneDefault"]!!.jsonPrimitive.content, Apps.EditorTabs.PHONE_DEFAULT)
        assertEquals(tabs["settingsMaxWidth"]!!.jsonPrimitive.int, Apps.EditorTabs.SETTINGS_MAX_WIDTH)
        assertEquals(tabs["search"]!!.jsonPrimitive.content, Apps.EditorTabs.SEARCH)
        // VAPP-177: the floating layout is gone.
        assertEquals(null, fixture["layout"])
        assertEquals(null, fixture["layoutCases"])
        val cases = fixture["editorTabCases"]!!.jsonArray.map { it.jsonObject }
        assert(cases.isNotEmpty())
        fun opt(case: JsonObject, key: String) = case[key]!!.let { if (it is JsonNull) null else it.jsonPrimitive.content }
        for (case in cases) {
            val pick = Apps.editorTab(opt(case, "requested"), case["phone"]!!.jsonPrimitive.content.toBooleanStrict())
            assertEquals(case.toString(), Apps.EditorTabPick(case["tab"]!!.jsonPrimitive.content, opt(case, "sheet")), pick)
        }
        assertEquals(true, Apps.isDraft(0))
        assertEquals(false, Apps.isDraft(1))
    }

    @Test
    fun `sortApps replays the fixture`() {
        for (case in fixture["sortAppsCases"]!!.jsonArray.map { it.jsonObject }) {
            val apps = case["apps"]!!.jsonArray.map { it.jsonObject }.map {
                Row(it["slug"]!!.jsonPrimitive.content, it["name"]!!.jsonPrimitive.content)
            }
            assertEquals(case["order"]!!.jsonArray.map { it.jsonPrimitive.content }, Apps.sortApps(apps).map { it.slug })
        }
    }

    @Test
    fun `the copy, preview and test ids match the fixture`() {
        val copy = fixture["copy"]!!.jsonObject
        fun c(key: String) = copy[key]!!.jsonPrimitive.content
        val menu = copy["menu"]!!.jsonObject
        val tabs = copy["tabs"]!!.jsonObject
        val expected = listOf(
            c("nav") to Apps.Copy.NAV, c("title") to Apps.Copy.TITLE, c("newApp") to Apps.Copy.NEW_APP,
            c("untitled") to Apps.Copy.UNTITLED, c("import") to Apps.Copy.IMPORT,
            c("versionsEmpty") to Apps.Copy.VERSIONS_EMPTY, c("draftPlaceholder") to Apps.Copy.DRAFT_PLACEHOLDER,
            c("starterTitle") to Apps.Copy.STARTER_TITLE,
            c("version") to Apps.Copy.VERSION, c("noVersion") to Apps.Copy.NO_VERSION,
            c("versions") to Apps.Copy.VERSIONS, c("head") to Apps.Copy.HEAD, c("noNote") to Apps.Copy.NO_NOTE,
            c("byAgent") to Apps.Copy.BY_AGENT, c("viewing") to Apps.Copy.VIEWING, c("restore") to Apps.Copy.RESTORE,
            c("restored") to Apps.Copy.RESTORED, c("restoredNote") to Apps.Copy.RESTORED_NOTE, c("empty") to Apps.Copy.EMPTY,
            c("publish") to Apps.Copy.PUBLISH, c("published") to Apps.Copy.PUBLISHED,
            c("publishedToast") to Apps.Copy.PUBLISHED_TOAST, c("publishNothing") to Apps.Copy.PUBLISH_NOTHING,
            c("migrating") to Apps.Copy.MIGRATING, c("migratingOf") to Apps.Copy.MIGRATING_OF, c("migrationFailed") to Apps.Copy.MIGRATION_FAILED,
            c("chatEmpty") to Apps.Copy.CHAT_EMPTY, c("previewEmpty") to Apps.Copy.PREVIEW_EMPTY,
            c("composerPlaceholder") to Apps.Copy.COMPOSER_PLACEHOLDER, c("noDevice") to Apps.Copy.NO_DEVICE,
            c("namePlaceholder") to Apps.Copy.NAME_PLACEHOLDER, c("descriptionPlaceholder") to Apps.Copy.DESCRIPTION_PLACEHOLDER,
            c("deleteTitle") to Apps.Copy.DELETE_TITLE,
            c("deleteBody") to Apps.Copy.DELETE_BODY, c("delete") to Apps.Copy.DELETE,
            menu["versions"]!!.jsonPrimitive.content to Apps.Copy.MENU_VERSIONS,
            menu["settings"]!!.jsonPrimitive.content to Apps.Copy.MENU_SETTINGS,
            menu["export"]!!.jsonPrimitive.content to Apps.Copy.MENU_EXPORT,
            menu["delete"]!!.jsonPrimitive.content to Apps.Copy.MENU_DELETE,
            menu["edit"]!!.jsonPrimitive.content to Apps.Copy.MENU_EDIT,
            c("runEmpty") to Apps.Copy.RUN_EMPTY,
            tabs["chat"]!!.jsonPrimitive.content to Apps.Copy.TAB_CHAT,
            tabs["preview"]!!.jsonPrimitive.content to Apps.Copy.TAB_PREVIEW,
            tabs["data"]!!.jsonPrimitive.content to Apps.Copy.TAB_DATA,
            tabs["versions"]!!.jsonPrimitive.content to Apps.Copy.TAB_VERSIONS,
            tabs["settings"]!!.jsonPrimitive.content to Apps.Copy.TAB_SETTINGS,
            c("preview") to Apps.Copy.PREVIEW,
            c("records") to Apps.Copy.RECORDS,
            c("recordsOne") to Apps.Copy.RECORDS_ONE, c("recordsOf") to Apps.Copy.RECORDS_OF,
            c("recordsNone") to Apps.Copy.RECORDS_NONE,
        )
        for ((want, got) in expected) assertEquals(want, got)
        assertEquals(copy["starters"]!!.jsonArray.map { it.jsonPrimitive.content }, Apps.Copy.STARTERS)
        // VAPP-176: no Rename, no create form.
        assertEquals(null, copy["create"])
        assertEquals(null, copy["rename"])
        assertEquals(setOf("versions", "settings", "export", "delete", "edit"), menu.keys)
        assertEquals(setOf("chat", "preview", "data", "versions", "settings"), tabs.keys)
        // VAPP-177: the floating layout's copy is gone.
        for (gone in listOf("dockToSide", "float", "working", "openEditor")) assertEquals(null, copy[gone])

        // Exponential ships no built-in apps (2026-10-24).
        assertEquals(null, fixture["builtin"])
        assertEquals(null, copy["builtin"])

        val preview = fixture["preview"]!!.jsonObject
        assertEquals(preview["radius"]!!.jsonPrimitive.int, Apps.Preview.RADIUS)
        assertEquals(preview["padding"]!!.jsonPrimitive.int, Apps.Preview.PADDING)
        assertEquals(preview["surfaceId"]!!.jsonPrimitive.content, Apps.Preview.SURFACE_ID)

        val ids = fixture["testIds"]!!.jsonObject
        val expectedIds = mapOf(
            "list" to Apps.TestIds.LIST, "row" to Apps.TestIds.ROW, "newApp" to Apps.TestIds.NEW_APP,
            "editor" to Apps.TestIds.EDITOR, "chat" to Apps.TestIds.CHAT, "preview" to Apps.TestIds.PREVIEW,
            "versions" to Apps.TestIds.VERSIONS, "versionRow" to Apps.TestIds.VERSION_ROW, "tabs" to Apps.TestIds.TABS,
            "composer" to Apps.TestIds.COMPOSER, "viewing" to Apps.TestIds.VIEWING,
            "importApp" to Apps.TestIds.IMPORT_APP, "starter" to Apps.TestIds.STARTER,
            "menu" to Apps.TestIds.MENU, "settings" to Apps.TestIds.SETTINGS,
            "versionPreview" to Apps.TestIds.VERSION_PREVIEW, "versionRestore" to Apps.TestIds.VERSION_RESTORE,
            "publish" to Apps.TestIds.PUBLISH, "versionPublish" to Apps.TestIds.VERSION_PUBLISH,
            "data" to Apps.TestIds.DATA,
            "previewToolbar" to Apps.TestIds.PREVIEW_TOOLBAR, "previewFrame" to Apps.TestIds.PREVIEW_FRAME,
            "previewTemplate" to Apps.TestIds.PREVIEW_TEMPLATE, "previewRunAs" to Apps.TestIds.PREVIEW_RUN_AS,
            "automations" to Apps.TestIds.AUTOMATIONS, "automationRow" to Apps.TestIds.AUTOMATION_ROW,
            "webhooks" to Apps.TestIds.WEBHOOKS, "webhookRow" to Apps.TestIds.WEBHOOK_ROW,
            "webhookSecret" to Apps.TestIds.WEBHOOK_SECRET,
            "previewScreens" to Apps.TestIds.PREVIEW_SCREENS,
            "run" to Apps.TestIds.RUN, "runMenu" to Apps.TestIds.RUN_MENU, "newMenu" to Apps.TestIds.NEW_MENU,
            "accounts" to Apps.TestIds.ACCOUNTS, "accountRow" to Apps.TestIds.ACCOUNT_ROW,
            "mcpTools" to Apps.TestIds.MCP_TOOLS, "mcpToolRow" to Apps.TestIds.MCP_TOOL_ROW,
            "policies" to Apps.TestIds.POLICIES, "policyRow" to Apps.TestIds.POLICY_ROW,
        )
        assertEquals(ids.keys, expectedIds.keys)
        for ((key, value) in expectedIds) assertEquals(ids[key]!!.jsonPrimitive.content, value)
    }

    @Test
    fun `the caps match the contract`() {
        val apps = contract["apps"]!!.jsonObject
        assertEquals(apps["slugMax"]!!.jsonPrimitive.int, Apps.SLUG_MAX)
        assertEquals(apps["nameMax"]!!.jsonPrimitive.int, Apps.NAME_MAX)
        assertEquals(apps["descriptionMax"]!!.jsonPrimitive.int, Apps.DESCRIPTION_MAX)
        assertEquals(apps["noteMax"]!!.jsonPrimitive.int, Apps.NOTE_MAX)
        assertEquals(apps["kindPattern"]!!.jsonPrimitive.content, Apps.KIND_PATTERN)
        assertEquals(apps["recordMaxBytes"]!!.jsonPrimitive.int, Apps.RECORD_MAX_BYTES)
        assertEquals(apps["recordKindsMax"]!!.jsonPrimitive.int, Apps.RECORD_KINDS_MAX)
    }

    @Test
    fun `the labels fill their slots`() {
        assertEquals("Viewing v3", Apps.viewing(3))
        assertEquals("Published v4", Apps.publishedToast(4))
        assertEquals("Restored v2 as v5", Apps.restored(2, 5))
        assertEquals("Delete Team dashboard?", Apps.deleteTitle("Team dashboard"))
    }

    @Test
    fun `previewTemplate replays the fixture`() {
        for (case in fixture["templateCases"]!!.jsonArray.map { it.jsonObject }) {
            val templates = case["templates"]!!.jsonArray.map { it.jsonPrimitive.content }
            val picked = case["picked"]!!.jsonPrimitive.let { if (it is JsonNull) null else it.content }
            val shown = case["shown"]!!.jsonPrimitive.let { if (it is JsonNull) null else it.content }
            assertEquals(case["list"]!!.jsonArray.map { it.jsonPrimitive.content }, Apps.previewTemplates(templates))
            assertEquals(shown, Apps.previewTemplate(templates, picked))
        }
    }

    // VAPP-218: the screens contract.
    @Test
    fun `screens replay the fixture`() {
        val screens = fixture["screens"]!!.jsonObject
        assertEquals(screens["sidebarWidth"]!!.jsonPrimitive.int, Apps.Screens.SIDEBAR_WIDTH)
        assertEquals(screens["reserved"]!!.jsonArray.map { it.jsonPrimitive.content }, Apps.Screens.RESERVED)
        fun strings(o: JsonObject) = o.mapValues { it.value.jsonPrimitive.content }
        val shells = fixture["screenShellCases"]!!.jsonArray.map { it.jsonObject }
        assert(shells.isNotEmpty())
        for (case in shells) {
            val want = case["shell"]!!.let { if (it is JsonNull) null else it.jsonPrimitive.content }
            val got = Apps.screenShell(case["screens"]!!.jsonPrimitive.int, case["phone"]!!.jsonPrimitive.content.toBooleanStrict())
            assertEquals(case.toString(), want, got?.wire)
        }
        val paths = fixture["screenPathCases"]!!.jsonArray.map { it.jsonObject }
        assert(paths.isNotEmpty())
        for (case in paths) {
            fun s(key: String) = case[key]!!.jsonPrimitive.content
            assertEquals(case.toString(), s("path"), Apps.screenPath(s("teamSlug"), s("appSlug"), s("screen"), strings(case["params"]!!.jsonObject)))
        }
        val queries = fixture["screenQueryCases"]!!.jsonArray.map { it.jsonObject }
        assert(queries.isNotEmpty())
        for (case in queries) {
            val got = Apps.screenParamsFromQuery(strings(case["query"]!!.jsonObject))
            assertEquals(case.toString(), strings(case["params"]!!.jsonObject), got)
            assertEquals(case.toString(), strings(case["params"]!!.jsonObject).keys.toList(), got.keys.toList())
        }
    }

    @Test
    fun `appMediaAllowed replays the fixture`() {
        val policy = fixture["mediaPolicy"]!!.jsonObject
        assertEquals(policy["dataMimePrefixes"]!!.jsonArray.map { it.jsonPrimitive.content }, Apps.MediaPolicy.DATA_MIME_PREFIXES)
        assertEquals(policy["deniedDataMimes"]!!.jsonArray.map { it.jsonPrimitive.content }, Apps.MediaPolicy.DENIED_DATA_MIMES)
        assertEquals(policy["attachmentPath"]!!.jsonPrimitive.content, Apps.MediaPolicy.ATTACHMENT_PATH)
        val origin = policy["origin"]!!.jsonPrimitive.content
        val cases = policy["cases"]!!.jsonArray.map { it.jsonObject }
        assert(cases.isNotEmpty())
        for (case in cases) {
            val src = case["src"]!!.jsonPrimitive.content
            assertEquals(case.toString(), case["expected"]!!.jsonPrimitive.content.toBooleanStrict(), Apps.appMediaAllowed(src, origin))
        }
        // A trailing slash on the origin changes nothing; no origin = relative + data only.
        assertEquals(true, Apps.appMediaAllowed("$origin/api/attachments/0b6c1a52-3e1f-4d7a-9c55-2f8e4a1b7c90", "$origin/"))
        assertEquals(false, Apps.appMediaAllowed("$origin/api/attachments/0b6c1a52-3e1f-4d7a-9c55-2f8e4a1b7c90", ""))
    }

    // VAPP-221: the app hosts' file upload (`appFiles`).
    @Test
    fun `appFiles replays targetCases and urlCases`() {
        val files = fixture["appFiles"]!!.jsonObject
        assertEquals(files["uploadPath"]!!.jsonPrimitive.content, Apps.AppFiles.UPLOAD_PATH)
        assertEquals(files["partName"]!!.jsonPrimitive.content, Apps.AppFiles.PART_NAME)
        assertEquals(files["failedVerb"]!!.jsonPrimitive.content, Apps.AppFiles.FAILED_VERB)
        assertEquals(files["failedThing"]!!.jsonPrimitive.content, Apps.AppFiles.FAILED_THING)
        assertEquals("/api/apps/a1b2/files", Apps.appUploadPath("a1b2"))
        fun value(e: kotlinx.serialization.json.JsonElement?): at.exponential.ui.json.JsonValue? =
            if (e == null || e is JsonNull) null else at.exponential.ui.json.JsonValue.parse(e.toString())
        val targets = files["targetCases"]!!.jsonArray.map { it.jsonObject }
        assert(targets.isNotEmpty())
        for (case in targets) {
            val components = case["components"]!!.jsonArray.map { value(it)!! }
            val expected = case["expected"]!!.let { if (it is JsonNull) null else it.jsonPrimitive.content }
            assertEquals(case.toString(), expected, Apps.appUploadTarget(components, case["name"]!!.jsonPrimitive.content))
        }
        val urls = files["urlCases"]!!.jsonArray.map { it.jsonObject }
        assert(urls.isNotEmpty())
        for (case in urls) {
            val got = Apps.withUploadedUrl(value(case["list"]), case["fileName"]!!.jsonPrimitive.content, case["url"]!!.jsonPrimitive.content)
            assertEquals(case.toString(), value(case["expected"])?.json, got?.json)
        }
    }
}
