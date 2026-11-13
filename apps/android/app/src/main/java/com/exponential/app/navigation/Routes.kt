package com.exponential.app.navigation

import com.exponential.app.domain.NeedsYou

/**
 * EXP-1291: the route strings every screen lane navigates by.
 *
 * - [HOME] = the composer page (the Agent tab, the app's landing root);
 *   seeded pushes ride `agentRoute(seed)` (`domain/AgentComposerSeed.kt`).
 * - [CHATS] = the Chats tab (it replaced Reviews).
 * - [chat] = ONE conversation screen for a chat root OR an agent root.
 * - [session] = the raw session view (the live transcript; was `steer/{id}`).
 * - [AGENTS_SETTINGS] / [agentSettings] / [AGENT_NEW] = Settings → Agents.
 */
object Routes {
    const val HOME = MainTabs.AGENT
    const val CHATS = MainTabs.CHATS

    /** #EXP-1388: the Chats tab's Archived page (pushed from its entry row). */
    const val CHATS_ARCHIVED = "chats-archived"

    /** #FEED-191 (needs-you.json): THE list, pushed from the Chats list's first row. */
    const val CHATS_NEEDS_YOU = "chats-needs-you"

    /** `tab` = the tab THE session screen opens on (#EXP-1732
     *  session-screen.json: `chat` · `issues` · `sessions` · `results`; legacy
     *  values `guide` / `plan` / ... map through `legacyTabs`); absent = Chat.
     *  `plan` = the plan card opened as the pushed item (#EXP-1662). `card` =
     *  the waiting card the Conversation centres (#FEED-191 needs-you.json). */
    const val CHAT_PATTERN = "chat/{sessionId}?tab={tab}&plan={plan}&card={card}"

    /** EXP-1635 (share-targets.json): STEP 1 of a share; New issue pushes [SHARE_COMPOSE]. */
    const val SHARE_TO = "share-to"
    const val SHARE_COMPOSE = "share-compose"

    const val CHAT_TAB_GUIDE = "guide"
    const val CHAT_TAB_PLAN = "plan"
    const val CHAT_TAB_RESULTS = "results"
    fun chat(rootSessionId: String, tab: String? = null): String =
        if (tab == null) "chat/$rootSessionId" else "chat/$rootSessionId?tab=$tab"

    const val SESSION_PATTERN = "session/{sessionId}"
    fun session(sessionId: String): String = "session/$sessionId"

    /** #EXP-1662 (plan-page.json WAYS IN): the chat [rootSessionId] open on
     *  its plan tab, on the plan card [messageId]. */
    fun chatPlan(rootSessionId: String, messageId: String): String =
        "chat/$rootSessionId?tab=$CHAT_TAB_PLAN&plan=$messageId"

    /** #FEED-191 (needs-you.json): the chat [rootSessionId] on its
     *  Conversation, centred on the waiting card [messageId]. */
    fun chatCard(rootSessionId: String, messageId: String): String =
        "chat/$rootSessionId?card=$messageId"

    /** #FEED-191 (needs-you.json): where a Needs you row opens, by `opens`. */
    fun needsYou(row: NeedsYou.Row): String = when (row.opens) {
        NeedsYou.Opens.Plan -> chatPlan(row.rootId, row.messageId)
        NeedsYou.Opens.Card -> chatCard(row.rootId, row.messageId)
        NeedsYou.Opens.Results -> chat(row.rootId, CHAT_TAB_RESULTS)
        NeedsYou.Opens.Chat -> chat(row.rootId)
    }

    /** #EXP-1381 (settings-nav.json): ONE team settings category's own page,
     *  `section` = a `SettingsNav` row id (General, Members, Labels, Boards,
     *  Agents, Repositories, Prompts, Devices). */
    const val TEAM_SETTINGS_PATTERN = "team-settings/{section}?device={device}"
    fun teamSettings(section: String): String = "team-settings/$section"

    /** #EXP-1534: the Devices page with one device's settings sheet open
     *  (`deviceRowId` = the `devices` row id an `agent_signed_out` row names). */
    fun deviceSettings(deviceRowId: String): String =
        "team-settings/${com.exponential.app.domain.SettingsNav.DEVICES}?device=$deviceRowId"

    /** #VAPP-185: Settings › MCP › Activity (settings-mcp.json `activityPage`). */
    const val MCP_ACTIVITY = "settings/mcp/activity"

    const val AGENTS_SETTINGS = "settings/agents"
    // NEW-2: an agent = a definition row (`agents.id`).
    const val AGENT_SETTINGS_PATTERN = "settings/agent/{agentId}"
    fun agentSettings(agentId: String): String = "settings/agent/$agentId"
    const val AGENT_NEW = "settings/agent-new"

    /** NEW-7: an agent's conversation = the CALLER's DM with it (the root
     *  session with that `agent_id` and `user_id` = me); none yet = the same
     *  screen empty, whose composer creates it. */
    const val AGENT_DM_PATTERN = "agent-dm/{agentId}"
    fun agentDm(agentId: String): String = "agent-dm/$agentId"

    /** NEW-7: Settings → Prompts (prompts replaced actions). */
    const val PROMPTS_SETTINGS = "settings/prompts"
    const val PROMPT_SETTINGS_PATTERN = "settings/prompt/{promptId}"
    fun promptSettings(promptId: String): String = "settings/prompt/$promptId"
    const val PROMPT_NEW = "settings/prompt-new"
    /** #EXP-1661: one skill's read-only page (Settings › Skills). */
    const val SKILL_SETTINGS_PATTERN = "settings/skill/{skillId}"
    fun skillSettings(skillId: String): String = "settings/skill/$skillId"

    /** VAPP-110: the selected team's Apps (reached from the Chats tab). */
    const val APPS = "apps"
    /** VAPP-110: one app's EDITOR (VAPP-242: only from a `…` › Edit app). */
    /** VAPP-218: `tab` = the editor tab to open on, `screen` + `params` are
     *  kept for the preview's screen request. */
    const val APP_PATTERN = "app/{appId}?tab={tab}&screen={screen}&params={params}"
    val APP_ARGS = listOf("tab", "screen", "params")
    fun app(appId: String): String = "app/$appId"

    /** VAPP-242 (apps.json THE RUN VIEW): an app as a member uses it, its
     *  PUBLISHED package; `screen` + `params` (a JSON object of strings) =
     *  the screen it opens on (an `appScreen` link). */
    const val APP_RUN_PATTERN = "app-run/{appId}?screen={screen}&params={params}"
    val APP_RUN_ARGS = listOf("screen", "params")
    fun appRun(appId: String): String = "app-run/$appId"
    fun appScreen(appId: String, screen: String, params: Map<String, String>): String {
        val json = kotlinx.serialization.json.JsonObject(params.mapValues { kotlinx.serialization.json.JsonPrimitive(it.value) }).toString()
        return "app-run/$appId?screen=${com.exponential.app.domain.encodeRouteValue(screen)}&params=${com.exponential.app.domain.encodeRouteValue(json)}"
    }

    /** #EXP-1804 (ci-run.json): THE run page of our own CI; `job` = the job
     *  it opens on (absent = the rule's pick, `CiRun.defaultJob`). */
    const val CI_RUN_PATTERN = "ci-run/{runId}?job={job}"
    fun ciRun(runId: String, job: String? = null): String =
        if (job.isNullOrEmpty()) "ci-run/$runId" else "ci-run/$runId?job=${com.exponential.app.domain.encodeRouteValue(job)}"
}
