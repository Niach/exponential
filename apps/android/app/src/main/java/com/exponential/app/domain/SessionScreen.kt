package com.exponential.app.domain

/**
 * #EXP-1732: THE session screen ×4 (a chat, an agent's or the Assistant's
 * conversation, an app's editor session): the header tabs per kind, the
 * gear, and the wide-screen beside rule. `fixtures/session-screen.json` (its
 * `$comment` is the spec; `SessionScreenTest` replays `cases`, `kindCases`
 * and `subtitleCases` whole); mirrors: TS `src/session-screen.ts`, desktop
 * `domain::session_screen`, iOS ExpCore `SessionScreen.swift`.
 *
 * Phones are always NARROW: the layout is `chat`, `tab` (the tab's body
 * replaces the chat) or `item` (a pushed page over the tab).
 */
object SessionScreen {

    object Copy {
        /** The tab labels, `copy.tabs` order. */
        val TABS: Map<String, String> = linkedMapOf(
            "chat" to "Chat",
            "issues" to "Issues",
            "sessions" to "Sessions",
            "results" to "Results",
            "preview" to "Preview",
            "data" to "Data",
            "versions" to "Versions",
        )
        const val SETTINGS = "Settings"
        const val BACK = "Back"
        const val SEPARATOR = " · "
        const val LINK_ISSUE = "Link or file an issue"

        /** `copy.status`: THE tree's lowercase status words. */
        object Status {
            const val NEEDS_INPUT = "needs input"
            const val WORKING = "working"
            const val QUEUED = "queued"
            const val DONE = "done"
            const val IDLE = "idle"
        }
    }

    object TestIds {
        const val TABS = "session-tabs"
        const val GEAR = "session-gear"
        const val BESIDE = "session-beside"
        const val PANE = "session-pane"
        const val ISSUES = "session-issues"
        const val SESSIONS = "session-sessions"

        /** One segment of [TABS] (the Android capture walks address them). */
        fun tab(tab: Tab): String = "session-tab-${tab.wire}"
    }

    /** `beside`: the wide split (never on a phone; replayed for parity). */
    object Beside {
        const val BREAKPOINT = 768
        const val CHAT = 5
        const val PANE = 7
        const val CHAT_MIN_WIDTH = 360
    }

    enum class Kind(val wire: String) {
        Session("session"),
        Agent("agent"),
        Assistant("assistant"),
        App("app"),
        ;

        companion object {
            /** Unknown = a chat. */
            fun of(wire: String?): Kind = entries.firstOrNull { it.wire == wire } ?: Session
        }
    }

    enum class Tab(val wire: String) {
        Chat("chat"),
        Issues("issues"),
        Sessions("sessions"),
        Results("results"),
        Preview("preview"),
        Data("data"),
        Versions("versions"),
        ;

        val label: String get() = Copy.TABS.getValue(wire)

        companion object {
            fun of(wire: String?): Tab? = entries.firstOrNull { it.wire == wire }
        }
    }

    enum class Open(val wire: String) {
        Issue("issue"),
        Session("session"),
        Plan("plan"),
        Settings("settings"),
        ;

        companion object {
            fun of(wire: String?): Open? = entries.firstOrNull { it.wire == wire }
        }
    }

    enum class Layout(val wire: String) { Chat("chat"), Beside("beside"), Tab("tab"), Item("item") }

    /** `kinds[kind]`. */
    data class KindSpec(val tabs: List<Tab>, val gear: Boolean, val wideDefault: Tab)

    val KINDS: Map<Kind, KindSpec> = linkedMapOf(
        Kind.Session to KindSpec(listOf(Tab.Chat, Tab.Issues, Tab.Sessions, Tab.Results), gear = false, wideDefault = Tab.Chat),
        Kind.Agent to KindSpec(listOf(Tab.Chat, Tab.Issues, Tab.Sessions, Tab.Results), gear = true, wideDefault = Tab.Chat),
        Kind.Assistant to KindSpec(listOf(Tab.Chat, Tab.Issues, Tab.Sessions, Tab.Results), gear = true, wideDefault = Tab.Chat),
        Kind.App to KindSpec(
            listOf(Tab.Chat, Tab.Preview, Tab.Data, Tab.Versions, Tab.Sessions, Tab.Results),
            gear = true,
            wideDefault = Tab.Preview,
        ),
    )

    /** `counted`: the tabs that carry a count. */
    val COUNTED: List<Tab> = listOf(Tab.Issues, Tab.Sessions, Tab.Results)

    /** `legacyTabs`: an older `?tab=` value → the tab it lands on. */
    val LEGACY_TABS: Map<String, String> = linkedMapOf(
        "conversation" to "chat",
        "issue" to "issues",
        "session" to "sessions",
        "guide" to "results",
        "stories" to "results",
        "plan" to "chat",
    )

    /** One segment: the label and its muted count (null = none, 0 included). */
    data class TabEntry(val tab: Tab, val label: String, val count: Int?)

    data class Counts(val issues: Int = 0, val sessions: Int = 0, val results: Int = 0)

    data class Screen(
        val tabs: List<TabEntry>,
        val active: Tab,
        val gear: Boolean,
        val open: Open?,
        val layout: Layout,
    )

    /** The screen's kind from its ROOT session (an app wins over its agent). */
    fun kind(agentId: String?, appId: String?, assistant: Boolean): Kind = when {
        !appId.isNullOrEmpty() -> Kind.App
        !agentId.isNullOrEmpty() -> if (assistant) Kind.Assistant else Kind.Agent
        else -> Kind.Session
    }

    /** The header's subtitle: `status · device · trigger`, blanks dropped. */
    fun subtitle(status: String?, device: String?, trigger: String?): String =
        listOf(status, device, trigger)
            .map { (it ?: "").trim() }
            .filter { it.isNotEmpty() }
            .joinToString(Copy.SEPARATOR)

    /** The kind's tabs (no counts). */
    fun tabs(kind: Kind): List<Tab> = KINDS.getValue(kind).tabs

    /** Whether the kind's header carries THE gear. */
    fun gear(kind: Kind): Boolean = KINDS.getValue(kind).gear

    /** The ×4 ALGORITHM (`$comment`); [kind] / [requested] / [open] are wire
     *  values (anything unknown falls back as the fixture says). */
    fun screen(kind: String?, wide: Boolean, requested: String?, open: String?, counts: Counts): Screen =
        screen(Kind.of(kind), wide, requested, open, counts)

    fun screen(kind: Kind, wide: Boolean, requested: String?, open: String?, counts: Counts): Screen {
        val spec = KINDS.getValue(kind)
        val tabs = spec.tabs.map { tab ->
            val value = when (tab) {
                Tab.Issues -> counts.issues
                Tab.Sessions -> counts.sessions
                Tab.Results -> counts.results
                else -> 0
            }
            TabEntry(tab, tab.label, if (tab in COUNTED && value > 0) value else null)
        }
        val req = requested?.let { LEGACY_TABS[it] ?: it }
        var opened = Open.of(open)
        if (opened == null && req == Open.Settings.wire) opened = Open.Settings
        if (opened == Open.Settings && !spec.gear) opened = null
        // ACTIVE FOLLOWS THE OPENED ITEM: a session → Sessions, an issue →
        // Issues, when the kind has that tab (a plan and settings keep it).
        val follows = when (opened) {
            Open.Session -> Tab.Sessions
            Open.Issue -> Tab.Issues
            else -> null
        }?.takeIf { it in spec.tabs }
        val asked = Tab.of(req)?.takeIf { it in spec.tabs }
        val active = follows ?: asked ?: if (wide) spec.wideDefault else Tab.Chat
        val layout = when {
            wide -> if (active != Tab.Chat || opened != null) Layout.Beside else Layout.Chat
            opened != null -> Layout.Item
            active == Tab.Chat -> Layout.Chat
            else -> Layout.Tab
        }
        return Screen(tabs, active, spec.gear, opened, layout)
    }

    /** One session of the tree for [status]. */
    data class StatusInput(val status: String, val busy: Boolean, val needsInput: Boolean, val root: Boolean)

    /** THE tree's status word (`statusCases`): needs input > working > queued
     *  (the root) > done (the root ended) > idle; an empty tree = idle. */
    fun status(sessions: List<StatusInput>): String {
        val live = sessions.filter { it.status != "ended" }
        val root = sessions.firstOrNull { it.root }
        return when {
            live.any { it.needsInput } -> Copy.Status.NEEDS_INPUT
            live.any { it.busy } -> Copy.Status.WORKING
            root?.status == "queued" -> Copy.Status.QUEUED
            root?.status == "ended" -> Copy.Status.DONE
            else -> Copy.Status.IDLE
        }
    }
}
