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

    const val THREAD_PATTERN = "thread/{sessionId}"
    fun thread(rootSessionId: String): String = "thread/$rootSessionId"

    const val SESSION_PATTERN = "session/{sessionId}"
    fun session(sessionId: String): String = "session/$sessionId"

    const val AGENTS_SETTINGS = "settings/agents"
    const val AGENT_SETTINGS_PATTERN = "settings/agent/{sessionId}"
    fun agentSettings(agentSessionId: String): String = "settings/agent/$agentSessionId"
    const val AGENT_NEW = "settings/agent-new"
}
