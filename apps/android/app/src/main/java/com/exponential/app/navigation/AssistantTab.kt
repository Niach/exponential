package com.exponential.app.navigation

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.navigation.NavHostController
import com.exponential.app.domain.AgentSettings
import com.exponential.app.domain.Chats
import com.exponential.app.ui.apps.ArgsStore
import com.exponential.app.ui.chat.AgentConversationScreen
import com.exponential.app.ui.chat.ChatScreen
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.SkeletonRows
import com.exponential.app.ui.components.aboveDock
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.settings.AssistantSettingsRouteViewModel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow

/** The viewer's Assistant and their conversation with it (null = none yet). */
data class AssistantRef(val agentId: String, val sessionId: String?)

/** Where the Assistant's chat opens: its tab, plan and card (a push, a Needs
 *  you row); [nonce] makes each landing a fresh one. */
data class AssistantLanding(
    val tab: String? = null,
    val plan: String? = null,
    val card: String? = null,
    val openSettings: Boolean = false,
    val nonce: Int = 0,
)

/**
 * #EXP-2109 (chats.json ASSISTANT): opening the Assistant from anywhere (its
 * Chats row, a push, a link) selects its TAB. Every route that names its chat
 * or its agent is caught here ([intercept]) instead of being pushed.
 */
object AssistantTab {
    @Volatile
    var ref: AssistantRef? = null

    private val _landing = MutableStateFlow(AssistantLanding())
    val landing: StateFlow<AssistantLanding> = _landing

    /** The landing a [route] means for [ref]; null = another screen. Pure. */
    fun landingOf(route: String, ref: AssistantRef?): AssistantLanding? {
        if (ref == null) return null
        val path = route.substringBefore('?')
        val query = route.substringAfter('?', "").split('&').filter { it.contains('=') }
            .associate { it.substringBefore('=') to it.substringAfter('=') }
        return when {
            ref.sessionId != null && path == "chat/${ref.sessionId}" ->
                AssistantLanding(query["tab"], query["plan"], query["card"])
            path == "agent-dm/${ref.agentId}" -> AssistantLanding(openSettings = query["open"] == "settings")
            else -> null
        }
    }

    /** True = the Assistant's tab took [route]. */
    fun intercept(nav: NavHostController, route: String, marker: TabSwitchMarker?): Boolean {
        val landing = landingOf(route, ref) ?: return false
        val bare = landing == AssistantLanding()
        if (!bare) _landing.value = landing.copy(nonce = _landing.value.nonce + 1)
        nav.selectTab(MainTabs.ASSISTANT, marker)
        return true
    }
}

/** Open [route], the Assistant's chat on its tab. */
fun NavHostController.open(route: String, marker: TabSwitchMarker? = null) {
    if (!AssistantTab.intercept(this, route, marker)) navigate(route)
}

/**
 * The Assistant TAB: its chat screen without a back control, the composer above
 * the docked bar (which hides while the composer has the keyboard).
 */
@Composable
fun AssistantTabScreen(
    ref: StateFlow<AssistantRef?>,
    onOpenSession: (sessionId: String) -> Unit,
    onOpenIssue: (issueId: String) -> Unit,
    onOpenChat: (rootSessionId: String) -> Unit,
    onNewIssue: (boardId: String, rootSessionId: String) -> Unit,
    onOpenApp: (appId: String) -> Unit,
    ensure: AssistantSettingsRouteViewModel = hiltViewModel(),
) {
    val ready by ensure.state.collectAsStateWithLifecycle()
    val assistant by ref.collectAsStateWithLifecycle()
    val landing by AssistantTab.landing.collectAsStateWithLifecycle()
    val raise = (Chats.Shell.Dock.RAISE + Chats.Shell.Dock.RING).dp
    Box(Modifier.fillMaxSize().aboveDock(extra = raise).testTag("assistant-tab")) {
        val agentId = assistant?.agentId ?: (ready as? AssistantSettingsRouteViewModel.State.Ready)?.agentId
        val sessionId = assistant?.sessionId
        when {
            sessionId != null -> key(sessionId, landing.nonce) {
                ArgsStore(
                    mapOf(
                        "sessionId" to sessionId,
                        "tab" to if (landing.openSettings) "settings" else landing.tab,
                        "plan" to landing.plan,
                        "card" to landing.card,
                    ),
                ) {
                    ChatScreen(
                        onBack = null,
                        onOpenSession = onOpenSession,
                        onOpenIssue = onOpenIssue,
                        onOpenChat = onOpenChat,
                        onNewIssue = onNewIssue,
                        onOpenApp = onOpenApp,
                    )
                }
            }
            agentId != null -> key(agentId, landing.nonce) {
                ArgsStore("agentId", agentId) {
                    AgentConversationScreen(
                        onBack = null,
                        // The synced conversation swaps in on its own ([ref]).
                        onOpenConversation = {},
                        openSettings = landing.openSettings,
                        onOpenApp = onOpenApp,
                    )
                }
            }
            ready == AssistantSettingsRouteViewModel.State.NoAssistant ->
                EmptyState(title = AgentSettings.Assistant.NO_DEVICE, icon = ExpIcons.settingsAgents)
            else -> SkeletonRows()
        }
    }
}
