package com.exponential.app.navigation

/**
 * EXP-1291 Threads v2: the route strings every screen lane navigates by.
 *
 * - [HOME] = the composer page (the Agent tab, the app's landing root);
 *   seeded pushes ride `agentRoute(seed)` (`domain/AgentComposerSeed.kt`).
 * - [THREADS] = the Threads tab (it replaced Reviews).
 * - [thread] = ONE conversation screen for a thread root OR an agent root.
 * - [session] = the raw session view (the live transcript; was `steer/{id}`).
 * - [AGENTS_SETTINGS] / [agentSettings] / [AGENT_NEW] = Settings → Agents.
 */
object Routes {
    const val HOME = MainTabs.AGENT
    const val THREADS = MainTabs.THREADS

    /** #EXP-1388: the Chats tab's Archived page (pushed from its entry row). */
    const val CHATS_ARCHIVED = "threads-archived"

    /** `tab` = the tab the thread opens on (`guide` for a `?view=guide`
     *  link); absent = the Conversation. */
    const val THREAD_PATTERN = "thread/{sessionId}?tab={tab}"
    const val THREAD_TAB_GUIDE = "guide"
    fun thread(rootSessionId: String, tab: String? = null): String =
        if (tab == null) "thread/$rootSessionId" else "thread/$rootSessionId?tab=$tab"

    const val SESSION_PATTERN = "session/{sessionId}"
    fun session(sessionId: String): String = "session/$sessionId"

    /** #EXP-1493: THE plan page (plan-page.json) of one plan card
     *  (`messageId`) a session posted, a pushed page. */
    const val PLAN_PAGE_PATTERN = "plan/{sessionId}/{messageId}"
    fun planPage(sessionId: String, messageId: String): String = "plan/$sessionId/$messageId"

    /** #EXP-1381 (settings-nav.json): ONE team settings category's own page,
     *  `section` = a `SettingsNav` row id (General, Members, Labels, Boards,
     *  Agents, Repositories, Prompts, Devices). */
    const val TEAM_SETTINGS_PATTERN = "team-settings/{section}"
    fun teamSettings(section: String): String = "team-settings/$section"

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

    /** VAPP-110: the selected team's Apps (reached from the Threads tab). */
    const val APPS = "apps"
    /** VAPP-110: one app's editor (`builtin` = the built-in Devices app). */
    const val APP_PATTERN = "app/{appId}"
    fun app(appId: String): String = "app/$appId"
}
