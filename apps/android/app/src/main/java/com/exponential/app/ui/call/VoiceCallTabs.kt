package com.exponential.app.ui.call

import android.os.SystemClock
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.domain.VoiceCall
import com.exponential.app.ui.components.GlassSegmentedControl
import kotlinx.coroutines.delay

// FEED-272 (voice-call.json `tabs`): THE Chat | Voice tabs on top of the
// call surface (the filled segmented control, equal segments, the same look
// x4). Voice = the call screen as before; Chat = the agent's main chat with
// the call docked as the pill's row ([CallChatBar], drawn by the chat screen
// in place of its header). Switching never ends, mutes or re-dials the call.

@Composable
fun CallTabs(active: VoiceCall.Tab, onSelect: (VoiceCall.Tab) -> Unit, modifier: Modifier = Modifier) {
    GlassSegmentedControl(
        options = VoiceCall.Tabs.ORDER,
        selected = active,
        label = VoiceCall.Tabs::label,
        onSelect = onSelect,
        testTag = VoiceCall.Tabs::testId,
        modifier = modifier.fillMaxWidth().testTag(VoiceCall.Tabs.TABS_ID),
    )
}

/**
 * The Chat tab's top: the tabs over the pill's row docked (mute, the red
 * end). [docked] = the call runs for this chat's agent on the Chat tab; the
 * chat screen draws this instead of its header then. Every agent chat
 * registers while it shows, so the host knows whether to open one.
 */
@Composable
fun rememberCallDocked(agentId: String?, viewModel: VoiceCallHostViewModel = hiltViewModel()): Boolean {
    val controller = viewModel.controller
    DisposableEffect(agentId) {
        if (agentId != null) controller.chatShown(agentId, true)
        onDispose { if (agentId != null) controller.chatShown(agentId, false) }
    }
    val ui by controller.ui.collectAsStateWithLifecycle()
    return agentId != null && ui.view != null && !ui.minimized && ui.tab == VoiceCall.Tab.Chat && ui.agentId == agentId
}

@Composable
fun CallChatBar(viewModel: VoiceCallHostViewModel = hiltViewModel()) {
    val controller = viewModel.controller
    val ui by controller.ui.collectAsStateWithLifecycle()
    val level by controller.level.collectAsStateWithLifecycle()
    val view = ui.view ?: return
    val answeredAt = ui.answeredAt
    val now by produceState(SystemClock.elapsedRealtime(), answeredAt) {
        while (answeredAt != null) {
            value = SystemClock.elapsedRealtime()
            delay(1_000L - (value - answeredAt) % 1_000L)
        }
    }
    CallChatBarContent(
        agent = ui.agent,
        agentIcon = ui.agentIcon,
        agentColor = ui.agentColor,
        view = view,
        timer = answeredAt?.let { VoiceCall.callTimer(now - it) },
        level = level,
        onTab = controller::showTab,
        onMute = controller::toggleMute,
        onEnd = controller::end,
        modifier = Modifier.statusBarsPadding(),
    )
}

/** The bar itself (the styleguide specimen draws it with a static row). */
@Composable
fun CallChatBarContent(
    agent: String,
    agentIcon: String?,
    agentColor: String?,
    view: VoiceCall.View,
    timer: String?,
    level: Float,
    onTab: (VoiceCall.Tab) -> Unit,
    onMute: () -> Unit,
    onEnd: () -> Unit,
    modifier: Modifier = Modifier,
    still: Boolean = false,
) {
    Column(
        modifier = modifier.fillMaxWidth().padding(start = 12.dp, end = 12.dp, top = 8.dp, bottom = 8.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        CallTabs(active = VoiceCall.Tab.Chat, onSelect = onTab)
        VoiceCallPill(
            agent = agent,
            agentIcon = agentIcon,
            agentColor = agentColor,
            view = view,
            timer = timer,
            level = level,
            bands = emptyList(),
            onMute = onMute,
            onEnd = onEnd,
            still = still,
            docked = true,
        )
    }
}
