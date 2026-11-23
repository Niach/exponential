package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-1291: Team settings → Agents is fixture-locked ×4 — every
 * `summaryCases` / `validationCases` / `accountValueCases` entry of `agent-settings.json` runs here
 * (TS `agent-settings.test.ts`), the validation output re-encoded in the
 * fixture's shape and compared WHOLE; the embedded copy and fields are proved
 * equal to the fixture's.
 */
class AgentSettingsTest {

    private val fixture: JsonObject by lazy {
        Json.parseToJsonElement(contractFixtureJson("agent-settings.json")).jsonObject
    }

    private fun JsonObject.str(key: String): String? =
        this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    @Test
    fun `every builtin event has a summary`() {
        for (event in DomainContract.eventBuiltinNames) {
            assertTrue(event, !AgentSettings.Copy.EVENTS[event].isNullOrEmpty())
        }
        assertEquals(DomainContract.eventBuiltinNames.toSet(), AgentSettings.Copy.EVENTS.keys)
    }

    @Test
    fun `every summary case matches the fixture`() {
        val cases = fixture.getValue("summaryCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val trigger = case.getValue("trigger").jsonObject
            val like = AgentSettings.TriggerLike(
                kind = trigger.str("kind")!!,
                enabled = trigger["enabled"]?.takeUnless { it is JsonNull }?.jsonPrimitive?.boolean,
                cron = trigger.str("cron"),
                tz = trigger.str("tz"),
                name = trigger.str("name"),
                record = trigger.str("record"),
                op = trigger.str("op"),
            )
            assertEquals(case.str("expected"), AgentSettings.triggerSummary(like))
            // The same text off the PARSED stored trigger (the Triggers rows).
            val parsed = parseAgentTrigger(trigger)
            assertTrue(trigger.toString(), parsed != null)
            if (parsed != null) assertEquals(case.str("expected"), AgentSettings.triggerSummary(parsed))
        }
    }

    @Test
    fun `every validation case matches the fixture`() {
        val cases = fixture.getValue("validationCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val form = case.getValue("form").jsonObject
            val actual = AgentSettings.validateAgentForm(
                AgentSettings.Form(
                    name = form.str("name")!!,
                    deviceId = form.str("deviceId"),
                    instructions = form.str("instructions")!!,
                ),
            )
            val encoded = JsonArray(
                actual.map { error ->
                    buildJsonObject {
                        put("field", error.field)
                        put("message", error.message)
                    }
                },
            )
            assertEquals(case.str("name"), case.getValue("expected"), encoded)
        }
    }

    @Test
    fun `every account value case matches the fixture`() {
        val cases = fixture.getValue("accountValueCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val input = case.getValue("input").jsonObject
            val options = input.getValue("options").jsonArray.map { it.jsonObject }.map { option ->
                AgentSettings.AccountValueOption(
                    id = option.str("id")!!,
                    agent = option.str("agent")!!,
                    email = option.str("email")!!,
                    isLastUsed = option.getValue("isLastUsed").jsonPrimitive.boolean,
                )
            }
            val actual = AgentSettings.agentAccountValue(options, input.str("account"), input.str("cli"))
            val encoded = actual?.let { picked ->
                buildJsonObject {
                    put("agent", picked.agent)
                    put("email", picked.email)
                }
            } ?: JsonNull
            assertEquals(case.str("name"), case.getValue("expected"), encoded)
        }
    }

    // #EXP-1793: the agent page's MCP servers row (`mcpRow`, `mcpValueCases`).
    @Test
    fun `every mcp value case matches the fixture`() {
        val cases = fixture.getValue("mcpValueCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val names = case.getValue("names").jsonArray.map { it.jsonPrimitive.content }
            assertEquals(case.str("name"), case.str("expected"), agentMcpServersValue(names))
        }
    }

    @Test
    fun `the mcp row equals the fixture`() {
        val row = fixture.getValue("mcpRow").jsonObject
        assertEquals(row.str("id"), AgentSettings.McpRow.ID)
        assertEquals(row.str("label"), AgentSettings.McpRow.LABEL)
        assertEquals(row.str("after"), AgentSettings.McpRow.AFTER)
        assertEquals(row.str("none"), AgentSettings.McpRow.NONE)
        assertEquals(row.str("max"), AgentSettings.McpRow.MAX.toString())
    }

    @Test
    fun `the fields equal the fixture`() {
        val fields = fixture.getValue("fields").jsonArray.map { it.jsonObject }
        assertEquals(fields.size, AgentSettings.FIELDS.size)
        fields.zip(AgentSettings.FIELDS).forEach { (json, field) ->
            assertEquals(json.str("key"), field.key)
            assertEquals(json.str("label"), field.label)
            assertEquals(json.str("control"), field.control)
            assertEquals(json.getValue("required").jsonPrimitive.boolean, field.required)
            assertEquals(json["ownerOnly"]?.jsonPrimitive?.boolean ?: false, field.ownerOnly)
        }
    }

    @Test
    fun `the copy equals the fixture`() {
        val copy = fixture.getValue("copy").jsonObject
        assertEquals(copy.str("title"), AgentSettings.Copy.TITLE)
        assertEquals(copy.str("newAgent"), AgentSettings.Copy.NEW_AGENT)
        assertEquals(copy.str("namePlaceholder"), AgentSettings.Copy.NAME_PLACEHOLDER)
        assertEquals(copy.str("instructionsPlaceholder"), AgentSettings.Copy.INSTRUCTIONS_PLACEHOLDER)
        assertEquals(copy.str("addTrigger"), AgentSettings.Copy.ADD_TRIGGER)
        assertEquals(copy.str("noSchedule"), AgentSettings.Copy.NO_SCHEDULE)
        assertEquals(copy.str("paused"), AgentSettings.Copy.PAUSED)
        assertEquals(copy.str("deleteConfirm"), AgentSettings.Copy.DELETE_CONFIRM)
        // NEW-2: a schedule's summary is the schedule picker's (SchedulePickerTest).
        assertTrue(copy["schedule"] == null)
        assertEquals(copy.str("event"), AgentSettings.Copy.EVENT)
        val events = copy.getValue("events").jsonObject
        assertEquals(events.keys, AgentSettings.Copy.EVENTS.keys)
        for ((key, value) in events) assertEquals(value.jsonPrimitive.content, AgentSettings.Copy.EVENTS[key])
        val errors = copy.getValue("errors").jsonObject
        assertEquals(errors.str("nameRequired"), AgentSettings.Copy.ERROR_NAME_REQUIRED)
        assertEquals(errors.str("nameTooLong"), AgentSettings.Copy.ERROR_NAME_TOO_LONG)
        assertEquals(errors.str("deviceRequired"), AgentSettings.Copy.ERROR_DEVICE_REQUIRED)
        assertEquals(errors.str("instructionsTooLong"), AgentSettings.Copy.ERROR_INSTRUCTIONS_TOO_LONG)
        assertEquals(copy.str("accountLastUsed"), AgentSettings.Copy.ACCOUNT_LAST_USED)
        val testIds = fixture.getValue("testIds").jsonObject
        assertEquals(testIds.str("list"), AgentSettings.TEST_ID_LIST)
        assertEquals(testIds.str("page"), AgentSettings.TEST_ID_PAGE)
    }

    @Test
    fun `an agent trigger rides the strict wire shape`() {
        val schedule = AgentTrigger(
            id = null,
            enabled = true,
            whenPart = TriggerWhen.Schedule(cron = "0 9 * * 1-5", tz = "Europe/Vienna"),
        )
        assertEquals(
            """{"enabled":true,"kind":"schedule","cron":"0 9 * * 1-5","tz":"Europe/Vienna"}""",
            schedule.toWireJson().toString(),
        )
        val utc = AgentTrigger(id = null, enabled = true, whenPart = TriggerWhen.Schedule(cron = "0 0 * * 0"))
        assertEquals("""{"enabled":true,"kind":"schedule","cron":"0 0 * * 0"}""", utc.toWireJson().toString())
        val event = AgentTrigger(id = "t-1", enabled = false, whenPart = TriggerWhen.Event(name = "pr.merged"))
        assertEquals(
            """{"id":"t-1","enabled":false,"kind":"event","name":"pr.merged"}""",
            event.toWireJson().toString(),
        )
        // …and reads back tolerant: one unreadable element is skipped.
        val raw = JsonArray(listOf(event.toWireJson(), JsonPrimitive("junk"))).toString()
        assertEquals(listOf(event), parseAgentTriggers(raw))
    }

    // NEW-2: the Events band's caption (web `eventEmitterLabel`).
    @Test
    fun eventEmitterLabelNamesTheSessionThePersonOrTheServer() {
        val title: (String) -> String? = { id -> if (id == "s1") "Fix the build" else null }
        val user: (String) -> String = { id -> if (id == "u1") "Ann" else id }
        assertEquals("Fix the build", AgentSettings.eventEmitterLabel("session", "s1", null, title, user))
        assertEquals("Session", AgentSettings.eventEmitterLabel("session", "s2", null, title, user))
        assertEquals("Session", AgentSettings.eventEmitterLabel("session", null, null, title, user))
        assertEquals("Ann", AgentSettings.eventEmitterLabel("user", null, "u1", title, user))
        assertEquals("Someone", AgentSettings.eventEmitterLabel("user", null, null, title, user))
        assertEquals("Exponential", AgentSettings.eventEmitterLabel("server", null, null, title, user))
    }

    private fun JsonObject.agentRows(key: String): List<AgentSettings.AgentRowLike> =
        getValue(key).jsonArray.map { it.jsonObject }.map { row ->
            AgentSettings.AgentRowLike(
                id = row.str("id")!!,
                name = row.str("name")!!,
                icon = row.str("icon"),
                color = row.str("color"),
                sortOrder = row["sortOrder"]?.jsonPrimitive?.content?.toDouble() ?: 0.0,
            )
        }

    @Test
    fun `every actor case matches the fixture (EXP-1383)`() {
        val cases = fixture.getValue("actor").jsonObject.getValue("actorCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val input = case.getValue("input").jsonObject
            val actual = AgentSettings.issueActor(input.str("actorAgentId"), input.str("actorUserId"), case.agentRows("agents"))
            val encoded = when (actual) {
                is AgentSettings.IssueActor.Agent -> buildJsonObject {
                    put("kind", "agent")
                    put("id", actual.id)
                    put("name", actual.name)
                    put("icon", actual.icon)
                    put("color", actual.color)
                }
                is AgentSettings.IssueActor.User -> buildJsonObject {
                    put("kind", "user")
                    put("id", actual.id)
                }
                null -> JsonNull
            }
            assertEquals(case.str("name"), case.getValue("expected"), encoded)
        }
    }

    @Test
    fun `every assignee option case matches the fixture (EXP-1383)`() {
        val assignee = fixture.getValue("assignee").jsonObject
        assertEquals(assignee.getValue("copy").jsonObject.str("unassigned"), AgentSettings.ASSIGNEE_UNASSIGNED_LABEL)
        val cases = assignee.getValue("optionCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val input = case.getValue("input").jsonObject
            val members = input.getValue("members").jsonArray.map { it.jsonObject.str("id")!! }
            val actual = AgentSettings.assigneeOptions(members, input.agentRows("agents"))
            val expected = case.getValue("expected").jsonArray.map { it.jsonPrimitive.content }
            assertEquals(case.str("name"), expected, actual)
        }
    }

    @Test
    fun `an assignee write sends both columns, an agent clears the person (EXP-1383)`() {
        assertEquals(AgentSettings.Assignee(null, "G1"), AgentSettings.Assignee("U1", "G1").write())
        assertEquals(AgentSettings.Assignee("U1", null), AgentSettings.Assignee("U1", null).write())
        assertEquals(AgentSettings.Assignee.NONE, AgentSettings.Assignee(null, null).write())
        // A solo team keeps its assignee control while it has agents.
        assertTrue(AgentSettings.hidesAssigneeControl(memberCount = 1, agentCount = 0))
        assertTrue(!AgentSettings.hidesAssigneeControl(memberCount = 1, agentCount = 1))
        assertTrue(!AgentSettings.hidesAssigneeControl(memberCount = 1, agentCount = 0, assigneeAgentId = "G1"))
        assertTrue(!AgentSettings.hidesAssigneeControl(memberCount = 2, agentCount = 0))
    }

    // S-3: the Assistant (`assistant.*Cases`, TS `agent-settings.test.ts`).
    private val assistant: JsonObject get() = fixture.getValue("assistant").jsonObject

    @Test
    fun `assistant copy, schedules and sections are the fixture's`() {
        val copy = assistant.getValue("copy").jsonObject
        assertEquals(copy.str("title"), AgentSettings.Assistant.TITLE)
        assertEquals(copy.str("name"), AgentSettings.Assistant.NAME)
        assertEquals(copy.str("runsOn"), AgentSettings.Assistant.RUNS_ON)
        assertEquals(copy.str("checkIn"), AgentSettings.Assistant.CHECK_IN)
        assertEquals(copy.str("voice"), AgentSettings.Assistant.VOICE)
        assertEquals(copy.str("noDevice"), AgentSettings.Assistant.NO_DEVICE)
        assertEquals(copy.str("deleteRefused"), AgentSettings.Assistant.DELETE_REFUSED)
        assertEquals(copy.str("scheduleRefused"), AgentSettings.Assistant.SCHEDULE_REFUSED)
        assertEquals(copy.str("promptFixed"), AgentSettings.Assistant.PROMPT_FIXED)
        assertEquals(copy.str("legacyInstructions"), AgentSettings.Assistant.LEGACY_INSTRUCTIONS)
        assertEquals(assistant.str("defaultSchedule"), AgentSettings.Assistant.DEFAULT_SCHEDULE)
        assertEquals(assistant.getValue("sections").jsonArray.map { it.jsonPrimitive.content }, AgentSettings.Assistant.SECTIONS)
        assertEquals(
            assistant.getValue("schedules").jsonArray.map {
                val o = it.jsonObject
                AgentSettings.Assistant.Schedule(o.str("id")!!, o.str("label")!!, o.str("cron"))
            },
            AgentSettings.Assistant.SCHEDULES,
        )
    }

    @Test
    fun `assistant runsOnCases`() {
        val cases = assistant.getValue("runsOnCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val devices = case.getValue("devices").jsonArray.map {
                val d = it.jsonObject
                AgentSettings.RunsOnDevice(d.str("deviceId")!!, d.str("kind"))
            }
            assertEquals(
                case.str("name"),
                case.getValue("expected").jsonArray.map { it.jsonPrimitive.content },
                AgentSettings.runsOnDeviceIds(
                    devices,
                    case.str("boundId"),
                    case.getValue("assistant").jsonPrimitive.boolean,
                ),
            )
        }
    }

    @Test
    fun `assistant modelValueCases`() {
        val cases = assistant.getValue("modelValueCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val input = case.getValue("input").jsonObject
            assertEquals(
                case.str("name"),
                case.str("expected"),
                AgentSettings.Assistant.modelValue(input.str("model"), input.str("cli")),
            )
        }
    }

    @Test
    fun `assistant scheduleCases`() {
        val cases = assistant.getValue("scheduleCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val triggers = case.getValue("triggers").jsonArray.map {
                val t = it.jsonObject
                AgentSettings.TriggerLike(
                    kind = t.str("kind")!!,
                    enabled = t["enabled"]?.takeUnless { e -> e is JsonNull }?.jsonPrimitive?.boolean,
                    cron = t.str("cron"),
                    tz = t.str("tz"),
                    name = t.str("name"),
                )
            }
            assertEquals(case.str("name"), case.str("expected"), AgentSettings.Assistant.scheduleOf(triggers))
        }
    }

    // #EXP-1846: the Check in (`checkIn.*Cases`, TS `agent-settings.test.ts`).
    private val checkIn: JsonObject get() = fixture.getValue("checkIn").jsonObject

    private fun JsonObject.triggers(key: String): List<AgentTrigger> =
        getValue(key).jsonArray.map { element ->
            val parsed = parseAgentTrigger(element)
            assertTrue(element.toString(), parsed != null)
            parsed!!
        }

    @Test
    fun `checkIn checkInCases`() {
        val cases = checkIn.getValue("checkInCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            assertEquals(case.str("name"), case.str("expected"), AgentSettings.checkInOf(case.triggers("triggers")))
        }
    }

    @Test
    fun `checkIn withCheckInCases`() {
        val cases = checkIn.getValue("withCheckInCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val actual = AgentSettings.withCheckIn(case.triggers("triggers"), case.str("preset")!!, case.str("tz"))
            assertEquals(case.str("name"), case.getValue("expected"), JsonArray(actual.map { it.toWireJson() }))
        }
    }

    @Test
    fun `checkIn listedCases`() {
        val cases = checkIn.getValue("listedCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            assertEquals(
                case.str("name"),
                case.getValue("expected").jsonArray.map { it.jsonPrimitive.content },
                AgentSettings.listedTriggers(case.triggers("triggers")).map { it.id },
            )
        }
    }

    @Test
    fun `the page sections and copy are the fixture's`() {
        val page = fixture.getValue("page").jsonObject
        val sections = page.getValue("sections").jsonArray.map { it.jsonObject }
        assertEquals(sections.map { it.str("id") }, AgentSettings.PAGE_SECTIONS)
        assertEquals(
            sections.first { it.str("id") == AgentSettings.SECTION_RUNS_ON }.getValue("rows").jsonArray.map { it.jsonPrimitive.content },
            AgentSettings.RUNS_ON_ROWS,
        )
        val teamPicks = sections.first { it.str("id") == AgentSettings.SECTION_TEAM_PICKS }
        assertEquals(teamPicks.getValue("rows").jsonArray.map { it.jsonPrimitive.content }, AgentSettings.TEAM_PICKS_ROWS)
        assertEquals("true", teamPicks.str("assistantOnly"))
        assertEquals("true", teamPicks.str("perTeam"))
        val copy = fixture.getValue("copy").jsonObject
        assertEquals(copy.str("triggers"), AgentSettings.Copy.TRIGGERS)
        assertEquals(copy.str("runsOn"), AgentSettings.Copy.RUNS_ON)
        assertEquals(copy.str("checkIn"), AgentSettings.Copy.CHECK_IN)
    }

    @Test
    fun `a check in rides the wire with its preset and every write keeps it`() {
        val checkIn = AgentTrigger(id = "t-ci", enabled = true, whenPart = TriggerWhen.Schedule("0 * * * *"), preset = "1h")
        assertEquals(
            """{"id":"t-ci","enabled":true,"kind":"schedule","cron":"0 * * * *","preset":"1h"}""",
            checkIn.toWireJson().toString(),
        )
        assertEquals(checkIn, parseAgentTrigger(checkIn.toWireJson()))
        // A toggle (copy) keeps the tag.
        assertEquals("1h", parseAgentTrigger(checkIn.copy(enabled = false).toWireJson())?.preset)
        // The Assistant's preset write is tagged too.
        assertEquals("1h", AgentSettings.Assistant.scheduleTriggers("1h").single().preset)
    }

    @Test
    fun `assistant every preset round-trips`() {
        for (schedule in AgentSettings.Assistant.SCHEDULES) {
            assertEquals(schedule.id, AgentSettings.Assistant.scheduleOfStored(AgentSettings.Assistant.scheduleTriggers(schedule.id)))
        }
    }

    @Test
    fun `assistant visibleCases`() {
        val cases = assistant.getValue("visibleCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val agent = case.getValue("agent").jsonObject
            assertEquals(
                case.str("name"),
                case.getValue("expected").jsonPrimitive.boolean,
                AgentSettings.Assistant.visibleAgent(agent.str("role"), agent.str("ownerId"), case.str("me")),
            )
        }
    }

    @Test
    fun `assistant orderCases`() {
        val cases = assistant.getValue("orderCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val agents = case.getValue("agents").jsonArray.map { it.jsonObject }
            val ordered = AgentSettings.Assistant.orderedAgents(agents, case.str("me")) {
                AgentSettings.Assistant.OrderLike(it.str("role"), it.str("ownerId"), it.str("name")!!, it.getValue("sortOrder").jsonPrimitive.content.toInt())
            }
            assertEquals(
                case.str("name"),
                case.getValue("expected").jsonArray.map { it.jsonPrimitive.content },
                ordered.map { it.str("id") },
            )
        }
    }

    @Test
    fun `assistant teamAgentsCases (EXP-1714)`() {
        val cases = assistant.getValue("teamAgentsCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val agents = case.getValue("agents").jsonArray.map { it.jsonObject }
            val listed = AgentSettings.Assistant.agentsInTeam(
                agents,
                case.str("teamId")!!,
                case.getValue("teamIds").jsonArray.map { it.jsonPrimitive.content },
                case.str("me"),
            ) { AgentSettings.Assistant.TeamAgentLike(it.str("teamId")!!, it.str("role"), it.str("ownerId")) }
            assertEquals(
                case.str("name"),
                case.getValue("expected").jsonArray.map { it.jsonPrimitive.content },
                listed.map { it.str("id") },
            )
        }
    }

    @Test
    fun `assistant teamPicks pickAgentCases (EXP-1988)`() {
        val cases = assistant.getValue("teamPicks").jsonObject.getValue("pickAgentCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val agents = case.getValue("agents").jsonArray.map { it.jsonObject }
            val picked = AgentSettings.Assistant.pickAgent(
                agents,
                case.str("teamId")!!,
                case.getValue("teamIds").jsonArray.map { it.jsonPrimitive.content },
                case.str("me"),
                case.str("pageAgentId"),
                id = { it.str("id")!! },
            ) { AgentSettings.Assistant.TeamAgentLike(it.str("teamId")!!, it.str("role"), it.str("ownerId")) }
            assertEquals(case.str("name"), case.str("expected"), picked)
        }
    }

    @Test
    fun `assistant teamPicks teamPickCases (EXP-1988)`() {
        val cases = assistant.getValue("teamPicks").jsonObject.getValue("teamPickCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            assertEquals(
                case.str("name"),
                case.getValue("expected").jsonArray.map { it.jsonPrimitive.content },
                AgentSettings.Assistant.teamPick(
                    case.getValue("ids").jsonArray.map { it.jsonPrimitive.content },
                    case.getValue("rowTeams").jsonObject.mapValues { it.value.jsonPrimitive.content },
                    case.str("teamId")!!,
                ),
            )
        }
    }

    @Test
    fun `assistant chatTeams cases (EXP-2079)`() {
        val cases = assistant.getValue("chatTeams").jsonObject.getValue("cases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            assertEquals(
                case.str("name"),
                case.getValue("expected").jsonArray.map { it.jsonPrimitive.content },
                AgentSettings.Assistant.chatTeams(
                    case.str("ownTeamId"),
                    case.getValue("syncedTeamIds").jsonArray.map { it.jsonPrimitive.content },
                    (case["pickedTeamIds"] as? kotlinx.serialization.json.JsonArray)?.map { it.jsonPrimitive.content },
                ),
            )
        }
    }

    @Test
    fun `assistant chatTeams deviceCases (EXP-2079)`() {
        val cases = assistant.getValue("chatTeams").jsonObject.getValue("deviceCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            assertEquals(
                case.str("name"),
                case.getValue("expected").jsonPrimitive.content.toBoolean(),
                AgentSettings.Assistant.ownDevices(
                    case.getValue("sessionTeamSynced").jsonPrimitive.content.toBoolean(),
                    case.getValue("starterIsViewer").jsonPrimitive.content.toBoolean(),
                ),
            )
        }
    }

    @Test
    fun `a team-scoped list keeps the viewer's personal sessions (EXP-1714)`() {
        val a = AgentSettings.Assistant
        assertTrue(a.inTeamOrPersonal("t-a", "u-x", "t-a", listOf("t-a"), "u-me"))
        assertTrue(a.inTeamOrPersonal("p-me", "u-me", "t-a", listOf("t-a"), "u-me"))
        assertTrue(!a.inTeamOrPersonal("p-x", "u-x", "t-a", listOf("t-a"), "u-me"))
        assertTrue(!a.inTeamOrPersonal("t-b", "u-me", "t-a", listOf("t-a", "t-b"), "u-me"))
    }

    // #EXP-1717: the team Coordinator (`coordinator.*`, TS `agent-settings.test.ts`).
    private val coordinator: JsonObject get() = fixture.getValue("coordinator").jsonObject

    @Test
    fun `coordinator copy and test id are the fixture's`() {
        val copy = coordinator.getValue("copy").jsonObject
        assertEquals(copy.str("label"), AgentSettings.Coordinator.LABEL)
        assertEquals(copy.str("none"), AgentSettings.Coordinator.NONE_LABEL)
        assertEquals(copy.str("refusedOwner"), AgentSettings.Coordinator.REFUSED_OWNER)
        assertEquals(copy.str("refusedAgent"), AgentSettings.Coordinator.REFUSED_AGENT)
        assertEquals(coordinator.getValue("testIds").jsonObject.str("row"), AgentSettings.Coordinator.TEST_ID_ROW)
    }

    @Test
    fun `coordinator optionCases`() {
        val cases = coordinator.getValue("optionCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val agents = case.getValue("agents").jsonArray.map { it.jsonObject }
            val options = AgentSettings.Coordinator.coordinatorOptions(agents) {
                AgentSettings.Coordinator.OptionLike(
                    it.str("id")!!,
                    it.str("name")!!,
                    it.str("role"),
                    it.getValue("sortOrder").jsonPrimitive.content.toDouble(),
                )
            }
            assertEquals(
                case.str("name"),
                case.getValue("expected").jsonArray.map { it.jsonPrimitive.content },
                options.map { it?.str("id") ?: AgentSettings.Coordinator.NONE },
            )
        }
    }

    @Test
    fun `coordinator valueCases`() {
        val cases = coordinator.getValue("valueCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val agents = case.getValue("agents").jsonArray.map { it.jsonObject }
            assertEquals(
                case.str("name"),
                case.str("expected"),
                AgentSettings.Coordinator.coordinatorOf(agents, case.str("teamId")!!, { it.str("id")!! }, { it.str("teamId")!! }, { it.str("role") }),
            )
        }
    }

    // #EXP-1735: the team's lead Assistant (`lead.*`, TS `agent-settings.test.ts`).
    private val lead: JsonObject get() = fixture.getValue("lead").jsonObject

    @Test
    fun `lead copy and test id are the fixture's`() {
        val copy = lead.getValue("copy").jsonObject
        assertEquals(copy.str("label"), AgentSettings.Lead.LABEL)
        assertEquals(copy.str("none"), AgentSettings.Lead.NONE_LABEL)
        assertEquals(copy.str("refusedOwner"), AgentSettings.Lead.REFUSED_OWNER)
        assertEquals(copy.str("refusedMember"), AgentSettings.Lead.REFUSED_MEMBER)
        assertEquals(copy.str("noAssistant"), AgentSettings.Lead.NO_ASSISTANT)
        assertEquals(lead.getValue("testIds").jsonObject.str("row"), AgentSettings.Lead.TEST_ID_ROW)
    }

    @Test
    fun `lead optionCases`() {
        val cases = lead.getValue("optionCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val members = case.getValue("members").jsonArray.map { it.jsonObject }
            val options = AgentSettings.Lead.leadOptions(members) {
                AgentSettings.Lead.MemberLike(it.str("userId")!!, it.str("name")!!)
            }
            assertEquals(
                case.str("name"),
                case.getValue("expected").jsonArray.map { it.jsonPrimitive.content },
                options.map { it?.str("userId") ?: AgentSettings.Lead.NONE },
            )
        }
    }

    // #EXP-1847: every agent's own voice (`voice.*`, TS `agent-settings.test.ts`).
    private val voice: JsonObject get() = fixture.getValue("voice").jsonObject

    @Test
    fun `voice rows, copy, clip upload and limits are the fixture's`() {
        assertEquals(voice.getValue("rows").jsonArray.map { it.jsonPrimitive.content }, AgentVoiceSection.ROWS)
        val page = fixture.getValue("page").jsonObject.getValue("sections").jsonArray.map { it.jsonObject }
        assertEquals(
            page.first { it.str("id") == AgentSettings.SECTION_VOICE }.getValue("rows").jsonArray.map { it.jsonPrimitive.content },
            AgentVoiceSection.ROWS,
        )
        val copy = voice.getValue("copy").jsonObject
        assertEquals(copy.str("label"), AgentVoiceSection.Copy.LABEL)
        assertEquals(copy.str("language"), AgentVoiceSection.Copy.LANGUAGE)
        assertEquals(copy.str("clip"), AgentVoiceSection.Copy.CLIP)
        assertEquals(copy.str("clipBuiltIn"), AgentVoiceSection.Copy.CLIP_BUILT_IN)
        assertEquals(copy.str("clipUpload"), AgentVoiceSection.Copy.CLIP_UPLOAD)
        assertEquals(copy.str("clipRemove"), AgentVoiceSection.Copy.CLIP_REMOVE)
        assertEquals(copy.str("transcript"), AgentVoiceSection.Copy.TRANSCRIPT)
        assertEquals(copy.str("transcriptPlaceholder"), AgentVoiceSection.Copy.TRANSCRIPT_PLACEHOLDER)
        assertEquals(copy.str("wakePhrase"), AgentVoiceSection.Copy.WAKE_PHRASE)
        assertEquals(copy.str("wakePhrasePlaceholder"), AgentVoiceSection.Copy.WAKE_PHRASE_PLACEHOLDER)
        val errors = copy.getValue("errors").jsonObject
        assertEquals(errors.str("clipType"), AgentVoiceSection.Copy.ERROR_CLIP_TYPE)
        assertEquals(errors.str("clipSize"), AgentVoiceSection.Copy.ERROR_CLIP_SIZE)
        val upload = voice.getValue("clipUpload").jsonObject
        assertEquals(upload.str("route"), AgentVoiceSection.ClipUpload.ROUTE)
        assertEquals(upload.getValue("accept").jsonArray.map { it.jsonPrimitive.content }, AgentVoiceSection.ClipUpload.ACCEPT)
        assertEquals(upload.str("maxBytes")!!.toLong(), AgentVoiceSection.ClipUpload.MAX_BYTES)
        val limits = voice.getValue("limits").jsonObject
        assertEquals(limits.str("transcript")!!.toInt(), AgentVoiceSection.Limits.TRANSCRIPT)
        assertEquals(limits.str("wakePhrase")!!.toInt(), AgentVoiceSection.Limits.WAKE_PHRASE)
        assertEquals(limits.str("wakePhraseMinLetters")!!.toInt(), AgentVoiceSection.Limits.WAKE_PHRASE_MIN_LETTERS)
        // The wake phrase error rule is voice.json's (Voice.wakePhraseError).
        assertEquals(AgentVoiceSection.Limits.WAKE_PHRASE, Voice.Limits.WAKE_PHRASE)
        assertEquals(AgentVoiceSection.Limits.WAKE_PHRASE_MIN_LETTERS, Voice.Limits.WAKE_PHRASE_MIN_LETTERS)
        assertEquals("/api/agents/a1/voice-clip", AgentVoiceSection.clipUploadPath("a1"))
        assertEquals("Hey Sloppy", AgentVoiceSection.wakePhrasePlaceholder("Sloppy"))
    }

    @Test
    fun `voice voiceOfCases`() {
        val cases = voice.getValue("voiceOfCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val agent = case.getValue("agent").jsonObject
            val member = case.getValue("member").jsonObject
            val resolved = agentVoiceOf(
                role = agent.str("role"),
                name = agent.str("name")!!,
                voice = AgentVoiceLike.of(agent.getValue("voice").jsonObject),
                member = MemberVoiceLike(
                    language = member.str("language")!!,
                    voiceFile = member.str("voiceFile")!!,
                    voiceTranscript = member.str("voiceTranscript")!!,
                    wakePhrase = member.str("wakePhrase")!!,
                ),
            )
            val encoded = buildJsonObject {
                put("language", resolved.language)
                put(
                    "clip",
                    when (val clip = resolved.clip) {
                        null -> JsonNull
                        is AgentVoiceClip.Attachment -> buildJsonObject { put("attachmentId", clip.attachmentId) }
                        is AgentVoiceClip.Path -> buildJsonObject { put("path", clip.path) }
                    },
                )
                put("clipLabel", resolved.clipLabel)
                put("transcript", resolved.transcript)
                put("wakePhrase", resolved.wakePhrase)
            }
            assertEquals(case.str("name"), case.getValue("expected"), encoded)
        }
    }

    @Test
    fun `a picked clip is refused when too large or not a WAV`() {
        assertEquals(null, AgentVoiceSection.clipRefusal("audio/wav", "me.wav", 1_000))
        assertEquals(null, AgentVoiceSection.clipRefusal("audio/x-wav", "me", 1_000))
        assertEquals(null, AgentVoiceSection.clipRefusal(null, "me.wav", null))
        assertEquals(null, AgentVoiceSection.clipRefusal("application/octet-stream", "clip", 10))
        assertEquals(null, AgentVoiceSection.clipRefusal("audio/vnd.wave", "Clip.WAV", 10))
        assertEquals(AgentVoiceSection.Copy.ERROR_CLIP_TYPE, AgentVoiceSection.clipRefusal("audio/mpeg", "me.mp3", 10))
        assertEquals(
            AgentVoiceSection.Copy.ERROR_CLIP_SIZE,
            AgentVoiceSection.clipRefusal("audio/wav", "me.wav", AgentVoiceSection.ClipUpload.MAX_BYTES + 1),
        )
        assertEquals(null, AgentVoiceSection.clipRefusal("audio/wav", "me.wav", AgentVoiceSection.ClipUpload.MAX_BYTES))
        assertEquals("audio/x-wav", AgentVoiceSection.clipContentType("audio/x-wav"))
        assertEquals("audio/wav", AgentVoiceSection.clipContentType("application/octet-stream"))
        assertEquals("audio/wav", AgentVoiceSection.clipContentType(null))
    }

    // #EXP-1850 (`detailCases`): a `run` trigger's second line, off the case
    // and off the PARSED stored trigger.
    @Test
    fun `every detail case matches the fixture`() {
        val cases = fixture.getValue("detailCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val trigger = case.getValue("trigger").jsonObject
            val parsed = parseAgentTrigger(trigger)
            assertTrue(trigger.toString(), parsed != null)
            assertEquals(trigger.toString(), case.str("expected"), AgentSettings.triggerDetail(parsed!!))
        }
        val copy = fixture.getValue("copy").jsonObject
        assertEquals(copy.str("record"), AgentSettings.Copy.RECORD)
        assertEquals(copy.str("webhook"), AgentSettings.Copy.WEBHOOK)
        assertEquals(copy.str("runs"), AgentSettings.Copy.RUNS)
        assertEquals(copy.str("handsOff"), AgentSettings.Copy.HANDS_OFF)
        assertEquals(
            copy.getValue("recordOps").jsonObject.mapValues { it.value.jsonPrimitive.content },
            AgentSettings.Copy.RECORD_OPS,
        )
    }

    // #EXP-1850: a readable trigger keeps prompt and run (and any key this
    // build has no field for) on every write; an edit keeps them too.
    @Test
    fun `record and webhook triggers read and every key rides the write`() {
        val raw = """[{"id":"a","enabled":true,"kind":"record","record":"request","op":"create","run":{"records":{"op":"upsert","kind":"log"}}},""" +
            """{"id":"b","enabled":false,"kind":"webhook","name":"stripe","prompt":"Look"},""" +
            """{"id":"c","enabled":true,"kind":"event","name":"pr.merged","prompt":"Review it","run":{"action":"x"}},""" +
            """{"id":"d","kind":"future","x":1}]"""
        val parsed = parseAgentTriggers(raw)
        assertEquals(listOf("a", "b", "c"), parsed.map { it.id })
        assertEquals(TriggerWhen.Record("request", "create"), parsed[0].whenPart)
        assertEquals(TriggerWhen.Webhook("stripe"), parsed[1].whenPart)
        assertEquals(
            """{"id":"a","enabled":true,"kind":"record","record":"request","op":"create","run":{"records":{"op":"upsert","kind":"log"}}}""",
            parsed[0].toWireJson().toString(),
        )
        assertEquals(
            """{"id":"b","enabled":false,"kind":"webhook","name":"stripe","prompt":"Look"}""",
            parsed[1].toWireJson().toString(),
        )
        // An edit (a new when-part over the stored trigger) keeps prompt and run.
        val edited = parsed[2].copy(whenPart = TriggerWhen.Event("pr.opened"))
        assertEquals(
            """{"id":"c","enabled":true,"kind":"event","name":"pr.opened","prompt":"Review it","run":{"action":"x"}}""",
            edited.toWireJson().toString(),
        )
        // Only the unknown kind is unreadable.
        assertEquals(listOf("d"), unreadableAgentTriggers(raw).map { it["id"]!!.jsonPrimitive.content })
        assertEquals(listOf("schedule", "event", "record", "webhook"), TriggerWhen.KINDS)
    }

    /** #EXP-1987: the Assistant's chat in a team the viewer does not sync,
     *  started by the viewer, is the member's own. */
    @Test
    fun `the assistant's chat in the personal team is the member's own`() {
        val teams = listOf("t-acme", "t-beta")
        assertTrue(AgentSettings.Assistant.ownChat("t-personal", "u1", teams, "u1"))
        assertEquals(false, AgentSettings.Assistant.ownChat("t-personal", "u2", teams, "u1"))
        assertEquals(false, AgentSettings.Assistant.ownChat("t-acme", "u1", teams, "u1"))
        assertEquals(false, AgentSettings.Assistant.ownChat("t-personal", "u1", emptyList(), "u1"))
        assertEquals(false, AgentSettings.Assistant.ownChat("t-personal", null, teams, null))
    }

    @Test
    fun `every groupChat chat case matches the fixture (EXP-1990)`() {
        val cases = fixture.getValue("groupChat").jsonObject.getValue("chatCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (c in cases) {
            val name = c.str("name")
            val agent = c.getValue("agent").jsonObject
            val sessions = c.getValue("sessions").jsonArray.map { it.jsonObject }
            val got = AgentSettings.agentChatOf(agent.str("agentId")!!, agent.str("groupChatId"), sessions, c.str("me")) {
                AgentSettings.ChatSessionLike(it.str("id")!!, it.str("parentId"), it.str("agentId"), it.str("userId"))
            }
            assertEquals(name, c.str("expected"), got?.str("id"))
        }
    }
}
