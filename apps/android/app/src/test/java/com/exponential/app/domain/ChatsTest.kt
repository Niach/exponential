package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Thread 58cb32df: Chats is fixture-locked ×4 — every case group of
 * `chats.json` (previewCases, timeCases, listCases, bubbleCases, menuCases,
 * agentMenuCases, replyCases, dividerCases, plainCases, windowCases, digestCases,
 * selectionCases, rangeCases, dotCases) runs here (TS
 * `chats.test.ts`, iOS `ChatsTests`, desktop `domain::chats`). Outputs are
 * re-encoded in the fixture's JSON shape and compared WHOLE; the embedded
 * copy is proved equal to the fixture's.
 */
class ChatsTest {

    private val fixture: JsonObject by lazy { Json.parseToJsonElement(contractFixtureJson("chats.json")).jsonObject }

    private fun JsonObject.str(key: String): String? =
        this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    private fun JsonObject.bool(key: String): Boolean = getValue(key).jsonPrimitive.boolean

    private fun JsonObject.list(key: String): List<JsonObject> =
        (this[key] as? JsonArray).orEmpty().map { it.jsonObject }

    private fun cases(key: String): List<JsonObject> = fixture.list(key).also { assertTrue(key, it.isNotEmpty()) }

    private fun nullable(value: String?): JsonElement = value?.let(::JsonPrimitive) ?: JsonNull

    private fun message(json: JsonObject) = SessionTree.Message(
        id = json.str("id")!!,
        sessionId = json.str("sessionId")!!,
        authorKind = json.str("authorKind")!!,
        authorId = json.str("authorId"),
        body = json.str("body")!!,
        card = json["card"] as? JsonObject,
        answeredAt = json.str("answeredAt"),
        consumedAt = json.str("consumedAt"),
        createdAt = json.str("createdAt")!!,
        authorSessionId = json.str("authorSessionId"),
    )

    private fun session(json: JsonObject) = SessionTree.Session(
        id = json.str("id")!!,
        teamId = json.str("teamId")!!,
        parentId = json.str("parentId"),
        rootId = json.str("rootId")!!,
        userId = json.str("userId")!!,
        status = json.str("status")!!,
        busy = json.bool("busy"),
        needsInput = json.bool("needsInput"),
        blocked = null,
        shared = json.bool("shared"),
        title = json.str("title"),
        guide = null,
        startedAt = json.str("startedAt"),
        endedAt = json.str("endedAt"),
        createdAt = json.str("createdAt")!!,
        updatedAt = json.str("updatedAt")!!,
        deviceId = json.str("deviceId"),
        agentId = json.str("agentId"),
    )

    private fun read(json: JsonObject) = SessionTree.SessionRead(json.str("sessionId")!!, json.str("readAt")!!, json.str("archivedAt"))

    private fun agent(json: JsonObject) = SessionTree.AgentRow(
        agentId = json.str("agentId")!!,
        teamId = json.str("teamId")!!,
        name = json.str("name")!!,
        icon = json.str("icon")!!,
        color = json.str("color")!!,
        busy = json.bool("busy"),
        needsYou = json.bool("needsYou"),
    )

    private fun groupKey(wire: String) = SessionTree.ThreadsGroupKey.entries.first { it.wire == wire }

    private fun listRow(json: JsonObject) = SessionTree.ThreadsListRow(
        sessionId = json.str("sessionId")!!,
        teamId = json.str("teamId")!!,
        title = json.str("title")!!,
        busy = json.bool("busy"),
        needsYou = json.bool("needsYou"),
        activityAt = json.str("activityAt")!!,
        deviceId = json.str("deviceId"),
        depth = json.getValue("depth").jsonPrimitive.int,
        parentId = json.str("parentId"),
        rootId = json.str("rootId")!!,
        unread = json.bool("unread"),
    )

    private fun encode(preview: Chats.ChatPreview): JsonObject = buildJsonObject {
        put("kind", preview.kind)
        put("text", preview.text)
    }

    private fun encode(row: Chats.ChatRow): JsonObject = buildJsonObject {
        put("sessionId", row.sessionId)
        put("teamId", row.teamId)
        put("title", row.title)
        put("deviceId", nullable(row.deviceId))
        put("group", row.group.wire)
        put("lead", row.lead.wire)
        put("preview", encode(row.preview))
        put("activityAt", row.activityAt)
        put("time", row.time)
        put("unreadCount", row.unreadCount)
        put("needsYou", row.needsYou)
        put("busy", row.busy)
        put("word", nullable(row.word?.wire))
    }

    private fun filterOf(wire: String) = Chats.ChatFilter.entries.first { it.wire == wire }

    @Test
    fun `every preview case matches the fixture`() {
        for (case in cases("previewCases")) {
            val input = case.getValue("input").jsonObject
            val actual = Chats.chatPreview(
                rootId = input.str("rootId")!!,
                messages = input.list("messages").map(::message),
                needsYou = input.bool("needsYou"),
                busy = input.bool("busy"),
                visible = input["visibleSessionIds"]?.jsonArray?.map { it.jsonPrimitive.content }?.toSet(),
            )
            assertEquals(case.str("name"), case.getValue("expected"), encode(actual))
        }
    }

    @Test
    fun `every time case matches the fixture`() {
        for (case in cases("timeCases")) {
            assertEquals(
                case.str("name"),
                case.str("expected"),
                Chats.chatTimeLabel(case.str("at")!!, case.str("now")!!, case.getValue("utcOffsetMinutes").jsonPrimitive.int),
            )
        }
    }

    @Test
    fun `every list case matches the fixture`() {
        for (case in cases("listCases")) {
            val input = case.getValue("input").jsonObject
            val groups = input.list("groups").map { group ->
                SessionTree.ThreadsListGroup(
                    key = groupKey(group.str("key")!!),
                    title = group.str("title")!!,
                    rows = group.list("rows").map(::listRow),
                )
            }
            val prStates = (input["prStates"] as? JsonObject).orEmpty().mapValues { (_, states) ->
                states.jsonArray.map { it.jsonPrimitive.content }
            }
            val words = (input["words"] as? JsonObject).orEmpty().mapValues { (_, word) ->
                SessionTree.ThreadRowWord.entries.first { it.wire == word.jsonPrimitive.content }
            }
            val actual = Chats.chatList(
                groups = groups,
                messages = input.list("messages").map(::message),
                reads = input.list("reads").map(::read),
                prStates = prStates,
                words = words,
                filter = filterOf(input.str("filter")!!),
                view = Chats.ChatListView.entries.first { it.wire == (input.str("view") ?: "list") },
                now = input.str("now")!!,
                utcOffsetMinutes = input.getValue("utcOffsetMinutes").jsonPrimitive.int,
            )
            val encoded = buildJsonObject {
                put("rows", JsonArray(actual.rows.map(::encode)))
                put(
                    "counts",
                    buildJsonObject {
                        put("needs_you", actual.counts.needsYou)
                        put("running", actual.counts.running)
                        put("unread", actual.counts.unread)
                    },
                )
                put("archived", actual.archived)
            }
            assertEquals(case.str("name"), case.getValue("expected"), encoded)
            // The chip labels read the counts.
            for (filter in Chats.FILTERS) {
                val n = actual.counts[filter]
                val label = Chats.Copy.FILTERS.getValue(filter)
                assertEquals(if (filter != Chats.ChatFilter.All && n > 0) "$label $n" else label, Chats.chatFilterLabel(filter, actual.counts))
            }
        }
    }

    @Test
    fun `every bubble case matches the fixture`() {
        for (case in cases("bubbleCases")) {
            val input = case.getValue("input").jsonObject
            val actual = Chats.agentBubbles(
                agents = input.list("agents").map(::agent),
                sessions = input.list("sessions").map(::session),
                messages = input.list("messages").map(::message),
                reads = input.list("reads").map(::read),
                me = input.str("me"),
            )
            val encoded = JsonArray(
                actual.map { bubble ->
                    buildJsonObject {
                        put("agentId", bubble.agent.agentId)
                        put("teamId", bubble.agent.teamId)
                        put("name", bubble.agent.name)
                        put("icon", bubble.agent.icon)
                        put("color", bubble.agent.color)
                        put("busy", bubble.agent.busy)
                        put("needsYou", bubble.agent.needsYou)
                        put("ring", nullable(bubble.ring?.wire))
                    }
                },
            )
            assertEquals(case.str("name"), case.getValue("expected"), encoded)
        }
    }

    @Test
    fun `every menu case matches the fixture`() {
        for (case in cases("menuCases")) {
            val input = case.getValue("input").jsonObject
            val actual = Chats.threadMenuEntries(input.str("rootStatus")!!, input.bool("canSteer"), input.bool("canDelete"))
            assertEquals(
                case.str("name"),
                case.getValue("expected").jsonArray.map { it.jsonPrimitive.content },
                actual.map { it.wire },
            )
        }
        for (case in cases("agentMenuCases")) {
            val input = case.getValue("input").jsonObject
            val actual = Chats.agentMenuEntries(input.bool("busy"), input.bool("isOwner"))
            assertEquals(
                case.str("name"),
                case.getValue("expected").jsonArray.map { it.jsonPrimitive.content },
                actual.map { it.wire },
            )
        }
    }

    private fun groups(input: JsonObject) = input.list("groups").map { group ->
        SessionTree.ThreadsListGroup(
            key = groupKey(group.str("key")!!),
            title = group.str("title")!!,
            rows = group.list("rows").map(::listRow),
        )
    }

    @Test
    fun `every selection, range and dot case matches the fixture (EXP-1388)`() {
        for (case in cases("selectionCases")) {
            val input = case.getValue("input").jsonObject
            val rows = input.list("rows").map {
                Chats.ChatSelectionRow(
                    sessionId = it.str("sessionId")!!,
                    rootStatus = it.str("rootStatus")!!,
                    unreadCount = it.getValue("unreadCount").jsonPrimitive.int,
                    canSteer = it.bool("canSteer"),
                    canDelete = it.bool("canDelete"),
                )
            }
            val view = Chats.ChatListView.entries.first { it.wire == (input.str("view") ?: "list") }
            val actual = JsonArray(
                Chats.chatSelectionActions(rows, view).map { entry ->
                    buildJsonObject {
                        put("action", entry.action.wire)
                        put("sessionIds", JsonArray(entry.sessionIds.map(::JsonPrimitive)))
                    }
                },
            )
            assertEquals(case.str("name"), case.getValue("expected"), actual)
        }
        for (case in cases("rangeCases")) {
            assertEquals(
                case.str("name"),
                case.getValue("expected").jsonArray.map { it.jsonPrimitive.content },
                Chats.chatSelectionRange(
                    case.getValue("order").jsonArray.map { it.jsonPrimitive.content },
                    case.str("anchor"),
                    case.str("target")!!,
                ),
            )
        }
        for (case in cases("dotCases")) {
            val input = case.getValue("input").jsonObject
            assertEquals(
                case.str("name"),
                case.bool("expected"),
                Chats.chatsUnread(groups(input), input.list("reads").map(::read)),
            )
        }
        assertEquals("Delete this chat?", Chats.chatDeleteTitle(1))
        assertEquals("Delete 3 chats?", Chats.chatDeleteTitle(3))
        assertEquals("2 of 5 chats couldn't be updated", Chats.chatSelectionFailedTitle(2, 5))
        assertEquals(Chats.Copy.TOAST_SELECTION_FAILED, Chats.chatSelectionFailedTitle(5, 5))
    }

    @Test
    fun `every reply case matches the fixture`() {
        for (case in cases("replyCases")) {
            val event = case.getValue("event").jsonObject
            val actual = Chats.surfaceReply(event["name"], event["context"])
            val expected = (case["expected"] as? JsonObject)?.str("text")
            assertEquals(case.str("name"), expected, actual)
        }
    }

    @Test
    fun `every divider and plain case matches the fixture`() {
        for (case in cases("dividerCases")) {
            assertEquals(case.str("body"), case.str("expected"), Chats.commandDivider(case.str("body")!!))
        }
        for (case in cases("plainCases")) {
            assertEquals(case.str("text"), case.str("expected"), Chats.plainLine(case.str("text")!!))
        }
    }

    @Test
    fun `every window case matches the fixture (review of #1218)`() {
        for (case in cases("windowCases")) {
            val start = Chats.transcriptWindowStart(
                rowCount = case.getValue("rowCount").jsonPrimitive.int,
                waitingIndexes = case.getValue("waitingIndexes").jsonArray.map { it.jsonPrimitive.int },
                pinnedStart = case["pinnedStart"]?.takeUnless { it is JsonNull }?.jsonPrimitive?.int,
                loadedSteps = case.getValue("loadedSteps").jsonPrimitive.int,
            )
            assertEquals(case.str("name"), case.getValue("start").jsonPrimitive.int, start)
        }
    }

    @Test
    fun `every digest case matches the fixture (thread e4a04d22)`() {
        for (case in cases("digestCases")) {
            val sessionId = case.str("sessionId")!!
            // Decoded the way the list reads it: the row's JSON text.
            val digest = case["digest"]?.takeUnless { it is JsonNull }?.toString()
            val actual = Chats.digestMessages(sessionId, Chats.parseDigest(digest)).map { m ->
                JsonObject(
                    mapOf(
                        "id" to JsonPrimitive(m.id),
                        "sessionId" to JsonPrimitive(m.sessionId),
                        "authorKind" to JsonPrimitive(m.authorKind),
                        "authorId" to nullable(m.authorId),
                        "authorSessionId" to nullable(m.authorSessionId),
                        "body" to JsonPrimitive(m.body),
                        "card" to (m.card ?: JsonNull),
                        "answeredAt" to nullable(m.answeredAt),
                        "consumedAt" to nullable(m.consumedAt),
                        "createdAt" to JsonPrimitive(m.createdAt),
                    ),
                )
            }
            assertEquals(case.str("name"), case.getValue("messages").jsonArray.toList(), actual)
        }
    }

    @Test
    fun `every agent hint case and the banner equal the fixture (EXP-1383)`() {
        val hint = fixture.getValue("agentHint").jsonObject
        val cases = hint.list("cases")
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val input = case.getValue("input").jsonObject
            assertEquals(
                case.str("name"),
                case.bool("expected"),
                Chats.agentInstructionsChanged(input.str("sessionStamp"), input.str("agentStamp")),
            )
        }
        val banner = hint.getValue("banner").jsonObject
        val row = Chats.AgentHint.banner
        assertEquals(banner.str("id"), row.id)
        assertEquals(banner.str("icon"), row.icon)
        assertEquals(banner.str("label"), row.label)
        assertEquals(banner.str("text"), row.text)
        assertEquals(banner.str("tone"), row.tone.wire)
        assertEquals(banner.str("action"), row.action)
        assertEquals(hint.str("sends"), Chats.AgentHint.SENDS)
    }

    @Test
    fun `the copy, shell, sends and surface card equal the fixture`() {
        val copy = fixture.getValue("copy").jsonObject
        assertEquals(copy.str("title"), Chats.Copy.TITLE)
        val tabs = copy.getValue("tabs").jsonObject
        assertEquals(tabs.keys.toList(), Chats.Copy.TABS.keys.toList())
        for ((key, label) in Chats.Copy.TABS) assertEquals(tabs.str(key), label)
        val filters = copy.getValue("filters").jsonObject
        assertEquals(filters.keys.toList(), Chats.Copy.FILTERS.keys.map { it.wire })
        for ((filter, label) in Chats.Copy.FILTERS) assertEquals(filters.str(filter.wire), label)
        val preview = copy.getValue("preview").jsonObject
        assertEquals(preview.str("working"), Chats.Copy.PREVIEW_WORKING)
        assertEquals(preview.str("question"), Chats.Copy.PREVIEW_QUESTION)
        assertEquals(preview.str("plan"), Chats.Copy.PREVIEW_PLAN)
        assertEquals(preview.str("you"), Chats.Copy.PREVIEW_YOU)
        assertEquals(preview.str("picture"), Chats.Copy.PREVIEW_PICTURE)
        assertEquals(preview.str("surface"), Chats.Copy.PREVIEW_SURFACE)
        assertEquals(preview.str("empty"), Chats.Copy.PREVIEW_EMPTY)
        assertEquals(preview.str("askQuestionLead"), Chats.Copy.PREVIEW_ASK_QUESTION_LEAD)
        assertEquals(preview.str("askPlanLead"), Chats.Copy.PREVIEW_ASK_PLAN_LEAD)
        assertEquals(preview.str("askPrivateQuestionLead"), Chats.Copy.PREVIEW_ASK_PRIVATE_QUESTION_LEAD)
        assertEquals(preview.str("askPrivatePlanLead"), Chats.Copy.PREVIEW_ASK_PRIVATE_PLAN_LEAD)
        assertEquals(preview.str("questionPrivate"), Chats.Copy.PREVIEW_QUESTION_PRIVATE)
        assertEquals(preview.str("planPrivate"), Chats.Copy.PREVIEW_PLAN_PRIVATE)
        assertEquals(preview.str("privateReplyLead"), Chats.Copy.PREVIEW_PRIVATE_REPLY_LEAD)
        assertEquals(preview.str("privateReportLead"), Chats.Copy.PREVIEW_PRIVATE_REPORT_LEAD)
        assertEquals(preview.str("privatePrLead"), Chats.Copy.PREVIEW_PRIVATE_PR_LEAD)
        assertEquals(16, preview.size)
        val time = copy.getValue("time").jsonObject
        assertEquals(time.str("yesterday"), Chats.Copy.TIME_YESTERDAY)
        assertEquals(time.getValue("weekdaysShort").jsonArray.map { it.jsonPrimitive.content }, Chats.Copy.WEEKDAYS_SHORT)
        assertEquals(time.getValue("monthsShort").jsonArray.map { it.jsonPrimitive.content }, Chats.Copy.MONTHS_SHORT)
        val menu = copy.getValue("menu").jsonObject
        assertEquals(menu.str("stop"), Chats.Copy.MENU_STOP)
        assertEquals(menu.str("stopAndDelete"), Chats.Copy.MENU_STOP_AND_DELETE)
        assertEquals(menu.str("delete"), Chats.Copy.MENU_DELETE)
        assertEquals(menu.str("compact"), Chats.Copy.MENU_COMPACT)
        assertEquals(menu.str("newConversation"), Chats.Copy.MENU_NEW_CONVERSATION)
        assertEquals(menu.str("settings"), Chats.Copy.MENU_SETTINGS)
        assertEquals(6, menu.size)
        for (entry in Chats.ThreadMenuEntry.entries) assertEquals(menu.str(entry.wire), Chats.threadMenuLabel(entry))
        for (entry in Chats.AgentMenuEntry.entries) assertEquals(menu.str(entry.wire), Chats.agentMenuLabel(entry))
        val confirm = copy.getValue("confirmDelete").jsonObject
        assertEquals(confirm.str("title"), Chats.Copy.CONFIRM_DELETE_TITLE)
        assertEquals(confirm.str("body"), Chats.Copy.CONFIRM_DELETE_BODY)
        val toasts = copy.getValue("toasts").jsonObject
        assertEquals(toasts.str("stopFailed"), Chats.Copy.TOAST_STOP_FAILED)
        assertEquals(toasts.str("deleteFailed"), Chats.Copy.TOAST_DELETE_FAILED)
        assertEquals(toasts.str("stopNotDelivered"), Chats.Copy.TOAST_STOP_NOT_DELIVERED)
        assertEquals(toasts.str("selectionFailed"), Chats.Copy.TOAST_SELECTION_FAILED)
        assertEquals(toasts.str("selectionPartial"), Chats.Copy.TOAST_SELECTION_PARTIAL)
        assertEquals(5, toasts.size)
        val selection = copy.getValue("selection").jsonObject
        assertEquals(selection.str("select"), Chats.Copy.SELECTION_SELECT)
        assertEquals(selection.str("count"), Chats.Copy.SELECTION_COUNT)
        assertEquals(selection.str("confirmDeleteMany"), Chats.Copy.SELECTION_CONFIRM_DELETE_MANY)
        for (action in Chats.ChatSelectionAction.entries) assertEquals(selection.str(action.wire), Chats.chatSelectionLabel(action))
        assertEquals(9, selection.size)
        val archived = copy.getValue("archived").jsonObject
        assertEquals(archived.str("entry"), Chats.Copy.ARCHIVED_ENTRY)
        assertEquals(archived.str("title"), Chats.Copy.ARCHIVED_TITLE)
        assertEquals(archived.str("empty"), Chats.Copy.ARCHIVED_EMPTY)
        assertEquals(3, archived.size)
        val dividers = copy.getValue("dividers").jsonObject
        assertEquals(dividers.str("new"), Chats.Copy.divider(Chats.DIVIDER_KIND_NEW))
        assertEquals(dividers.str("compact"), Chats.Copy.divider(Chats.DIVIDER_KIND_COMPACT))
        assertEquals(copy.str("superseded"), Chats.Copy.SUPERSEDED)
        assertEquals(copy.str("superseded"), AskDock.Copy.SUPERSEDED)
        val chips = copy.getValue("boardChips").jsonObject
        assertEquals(chips.str("all"), Chats.Copy.BOARD_CHIP_ALL)
        assertEquals(chips.str("mine"), Chats.Copy.BOARD_CHIP_MINE)
        assertEquals(chips.str("drafts"), Chats.Copy.BOARD_CHIP_DRAFTS)
        val team = copy.getValue("team").jsonObject
        assertEquals(team.str("meta"), Chats.Copy.TEAM_META)
        assertEquals(team.str("switch"), Chats.Copy.TEAM_SWITCH)
        assertEquals(copy.str("inbox"), Chats.Copy.INBOX)
        assertEquals(copy.str("loadEarlier"), Chats.Copy.LOAD_EARLIER)
        assertEquals(copy.str("working"), Chats.Copy.WORKING)
        val headers = copy.getValue("headers").jsonObject
        assertEquals(headers.keys.toList(), Chats.Copy.HEADERS.keys.toList())
        for ((key, title) in Chats.Copy.HEADERS) assertEquals(headers.str(key), title)
        assertEquals(copy.getValue("boardMenu").jsonObject.str("newBoard"), Chats.Copy.BOARD_MENU_NEW_BOARD)
        assertEquals(1, copy.getValue("boardMenu").jsonObject.size)
        assertEquals(
            setOf(
                "title", "tabs", "headers", "boardMenu", "filters", "preview", "time", "menu", "confirmDelete",
                "toasts", "dividers", "superseded", "boardChips", "team", "inbox", "loadEarlier", "working",
                "selection", "archived",
            ),
            copy.keys,
        )

        val shell = fixture.getValue("shell").jsonObject
        assertEquals(shell.getValue("tabs").jsonArray.map { it.jsonPrimitive.content }, Chats.Shell.TABS)
        val icons = shell.getValue("tabIcons").jsonObject
        assertEquals(icons.keys, Chats.Shell.TAB_ICONS.keys)
        for ((tab, icon) in Chats.Shell.TAB_ICONS) assertEquals(icons.str(tab), icon)
        assertEquals(shell.getValue("boardChips").jsonArray.map { it.jsonPrimitive.content }, Chats.Shell.BOARD_CHIPS)
        val bar = shell.getValue("bar").jsonObject
        assertEquals(bar.getValue("height").jsonPrimitive.int, Chats.Shell.Bar.HEIGHT)
        assertEquals(bar.getValue("padding").jsonPrimitive.int, Chats.Shell.Bar.PADDING)
        assertEquals(bar.getValue("gap").jsonPrimitive.int, Chats.Shell.Bar.GAP)
        assertEquals(bar.getValue("tabMinWidth").jsonPrimitive.int, Chats.Shell.Bar.TAB_MIN_WIDTH)
        assertEquals(bar.getValue("tabPaddingX").jsonPrimitive.int, Chats.Shell.Bar.TAB_PADDING_X)
        val drag = bar.getValue("drag").jsonObject
        assertEquals(drag.str("lensScale")!!.toFloat(), Chats.Shell.Bar.Drag.LENS_SCALE)
        assertEquals(drag.getValue("cancelDistance").jsonPrimitive.int, Chats.Shell.Bar.Drag.CANCEL_DISTANCE)
        assertEquals(setOf("height", "padding", "gap", "tabMinWidth", "tabPaddingX", "drag"), bar.keys)
        val boardTab = shell.getValue("boardTab").jsonObject
        assertEquals(boardTab.str("caret"), Chats.Shell.BoardTab.CARET)
        assertEquals(boardTab.getValue("caretSize").jsonPrimitive.int, Chats.Shell.BoardTab.CARET_SIZE)
        val boardMenu = shell.getValue("boardMenu").jsonObject
        assertEquals(boardMenu.getValue("rowHeight").jsonPrimitive.int, Chats.Shell.BoardMenu.ROW_HEIGHT)
        assertEquals(boardMenu.getValue("maxVisibleRows").jsonPrimitive.int, Chats.Shell.BoardMenu.MAX_VISIBLE_ROWS)
        assertEquals(boardMenu.getValue("radius").jsonPrimitive.int, Chats.Shell.BoardMenu.RADIUS)
        val menuMotion = boardMenu.getValue("motion").jsonObject
        assertEquals(menuMotion.str("open"), Chats.Shell.BoardMenu.OPEN)
        assertEquals(menuMotion.str("openEase"), Chats.Shell.BoardMenu.OPEN_EASE)
        assertEquals(menuMotion.str("close"), Chats.Shell.BoardMenu.CLOSE)
        assertEquals(menuMotion.str("closeEase"), Chats.Shell.BoardMenu.CLOSE_EASE)
        val header = shell.getValue("header").jsonObject
        assertEquals(header.getValue("pages").jsonArray.map { it.jsonPrimitive.content }, Chats.Shell.Header.PAGES)
        assertEquals(header.getValue("buttons").jsonArray.map { it.jsonPrimitive.content }, Chats.Shell.Header.BUTTONS)
        assertEquals(header.getValue("circle").jsonPrimitive.int, Chats.Shell.Header.CIRCLE)
        assertEquals(shell.getValue("leadDisc").jsonPrimitive.int, Chats.Shell.LEAD_DISC)
        assertEquals(shell.getValue("selectBadge").jsonPrimitive.int, Chats.Shell.SELECT_BADGE)
        assertEquals(shell.getValue("selectBadgeRing").jsonPrimitive.int, Chats.Shell.SELECT_BADGE_RING)
        assertEquals(shell.getValue("selectBadgeGlyph").jsonPrimitive.int, Chats.Shell.SELECT_BADGE_GLYPH)
        assertEquals(shell.getValue("selectBadgeRadius").jsonPrimitive.int, Chats.Shell.SELECT_BADGE_RADIUS)
        assertEquals(shell.getValue("archivedGlyph").jsonPrimitive.int, Chats.Shell.ARCHIVED_GLYPH)
        assertEquals(shell.getValue("bubble").jsonPrimitive.int, Chats.Shell.BUBBLE)
        assertEquals(shell.getValue("bubbleRing").jsonPrimitive.int, Chats.Shell.BUBBLE_RING)
        assertEquals(fixture.getValue("filters").jsonArray.map { it.jsonPrimitive.content }, Chats.FILTERS.map { it.wire })

        val ids = fixture.getValue("testIds").jsonObject
        assertEquals(ids.str("chatRow"), Chats.TestIds.CHAT_ROW)
        assertEquals(ids.str("chatFilter"), Chats.TestIds.CHAT_FILTER)
        assertEquals(ids.str("agentBubble"), Chats.TestIds.AGENT_BUBBLE)
        assertEquals(ids.str("inboxButton"), Chats.TestIds.INBOX_BUTTON)
        assertEquals(ids.str("tabTeam"), Chats.TestIds.TAB_TEAM)
        assertEquals(ids.str("boardChip"), Chats.TestIds.BOARD_CHIP)
        assertEquals(ids.str("surfaceCard"), Chats.TestIds.SURFACE_CARD)
        assertEquals(ids.str("divider"), Chats.TestIds.DIVIDER)
        assertEquals(ids.str("tabBoard"), Chats.TestIds.TAB_BOARD)
        assertEquals(ids.str("boardMenu"), Chats.TestIds.BOARD_MENU)
        assertEquals(ids.str("boardMenuRow"), Chats.TestIds.BOARD_MENU_ROW)
        assertEquals(ids.str("pageHeader"), Chats.TestIds.PAGE_HEADER)
        assertEquals(ids.str("searchButton"), Chats.TestIds.SEARCH_BUTTON)
        assertEquals(ids.str("chatArchived"), Chats.TestIds.CHAT_ARCHIVED)
        assertEquals(ids.str("selectionBar"), Chats.TestIds.SELECTION_BAR)
        assertEquals(17, ids.size)
        assertEquals(ids.str("loadEarlier"), Chats.TestIds.LOAD_EARLIER)
        assertEquals(ids.str("jumpToBottom"), Chats.TestIds.JUMP_TO_BOTTOM)

        val transcript = fixture.getValue("transcript").jsonObject
        assertEquals(transcript.getValue("window").jsonPrimitive.int, Chats.Transcript.WINDOW)
        assertEquals(transcript.getValue("windowStep").jsonPrimitive.int, Chats.Transcript.WINDOW_STEP)
        assertEquals(transcript.getValue("followSlack").jsonPrimitive.int, Chats.Transcript.FOLLOW_SLACK)
        assertEquals(3, transcript.size)

        val sends = fixture.getValue("agentMenu").jsonObject.getValue("sends").jsonObject
        assertEquals(sends.str("compact"), Chats.agentMenuSend(Chats.AgentMenuEntry.Compact))
        assertEquals(sends.str("newConversation"), Chats.agentMenuSend(Chats.AgentMenuEntry.NewConversation))
        assertEquals(2, sends.size)

        val surface = fixture.getValue("surfaceCard").jsonObject
        assertEquals(surface.str("replyEvent"), Chats.SurfaceCard.REPLY_EVENT)
        assertEquals(surface.getValue("surfaceMaxBytes").jsonPrimitive.int, Chats.SurfaceCard.MAX_BYTES)
        assertEquals(surface.getValue("surfaceMaxComponents").jsonPrimitive.int, Chats.SurfaceCard.MAX_COMPONENTS)
        val enter = surface.getValue("enter").jsonObject
        val frame = enter.getValue("frame").jsonObject
        assertEquals(frame.str("fromOpacity")!!.toFloat(), Chats.SurfaceCard.Frame.FROM_OPACITY)
        assertEquals(frame.str("fromTranslateY")!!.toFloat(), Chats.SurfaceCard.Frame.FROM_TRANSLATE_Y)
        assertEquals(frame.str("fromScale")!!.toFloat(), Chats.SurfaceCard.Frame.FROM_SCALE)
        assertEquals(frame.str("duration"), Chats.SurfaceCard.Frame.DURATION)
        assertEquals(frame.str("ease"), Chats.SurfaceCard.Frame.EASE)
        val children = enter.getValue("children").jsonObject
        assertEquals(children.str("fromOpacity")!!.toFloat(), Chats.SurfaceCard.Children.FROM_OPACITY)
        assertEquals(children.str("fromTranslateY")!!.toFloat(), Chats.SurfaceCard.Children.FROM_TRANSLATE_Y)
        assertEquals(children.str("duration"), Chats.SurfaceCard.Children.DURATION)
        assertEquals(children.str("ease"), Chats.SurfaceCard.Children.EASE)
        assertEquals(children.getValue("staggerMs").jsonPrimitive.int, Chats.SurfaceCard.Children.STAGGER_MS)
        assertEquals(children.getValue("maxStaggered").jsonPrimitive.int, Chats.SurfaceCard.Children.MAX_STAGGERED)
        assertEquals(children.getValue("startAfterMs").jsonPrimitive.int, Chats.SurfaceCard.Children.START_AFTER_MS)
        val glow = enter.getValue("glow").jsonObject
        assertEquals(glow.str("fromOpacity")!!.toFloat(), Chats.SurfaceCard.Glow.FROM_OPACITY)
        assertEquals(glow.str("duration"), Chats.SurfaceCard.Glow.DURATION)
        assertEquals(glow.str("ease"), Chats.SurfaceCard.Glow.EASE)
    }

    @Test
    fun `the tabs share the capsule and only the widest give way`() {
        // Room to spare: each keeps its own width plus an even share.
        assertEquals(listOf(110f, 70f, 60f, 80f), Chats.Shell.Bar.tabWidths(listOf(100f, 60f, 50f, 70f), 320f))
        // Exactly full: nothing moves.
        assertEquals(listOf(100f, 60f), Chats.Shell.Bar.tabWidths(listOf(100f, 60f), 160f))
        // Too narrow: the widest are capped first, the narrow ones keep theirs.
        assertEquals(listOf(70f, 60f, 50f, 70f), Chats.Shell.Bar.tabWidths(listOf(100f, 60f, 50f, 90f), 250f))
        assertEquals(emptyList<Float>(), Chats.Shell.Bar.tabWidths(emptyList(), 100f))
    }

    @Test
    fun `the contract carries the new builtin and card kind`() {
        assertTrue(DomainContract.steerCommandNames.contains("new"))
        assertEquals("surface", DomainContract.messageCardKindSurface)
    }
}
