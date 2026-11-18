import ExpCore
import ExpUI
import SwiftUI

// FEED-272 (voice-call.json `tabs`): THE Chat | Voice tabs on top of the
// call surface (the filled segmented control, equal segments, the same look
// x4). Voice = the call screen as before; Chat = the agent's main chat with
// the call docked as the pill's row ([CallChatBar], drawn by the chat screen
// in place of its header). Switching never ends, mutes or re-dials the call.

struct CallTabs: View {
    let active: VoiceCall.Tab
    let onSelect: (VoiceCall.Tab) -> Void

    var body: some View {
        GlassSegmentedControl(
            options: VoiceCall.Tabs.order,
            selection: active,
            label: VoiceCall.Tabs.label,
            identifier: VoiceCall.Tabs.testId,
            onSelect: onSelect
        )
        .frame(maxWidth: .infinity)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(VoiceCall.Tabs.tabsId)
    }
}

/// The Chat tab's top: the tabs over the pill's row docked (mute, the red
/// end), live off the app's one call.
struct CallChatBar: View {
    var client = VoiceCallClient.shared

    var body: some View {
        if let view = client.view {
            CallChatBarContent(
                view: view,
                callee: client.callee,
                timer: .since(client.answeredAt),
                level: client.level,
                onTab: { client.showTab($0) },
                onMute: { client.toggleMute() },
                onEnd: { client.end() }
            )
        }
    }
}

/// The bar itself (the styleguide specimen draws it with a still row).
struct CallChatBarContent: View {
    let view: VoiceCall.View
    let callee: VoiceCallCallee
    var timer: VoiceCallSurface.Timer
    var level: Double
    var animated: Bool = true
    var onTab: (VoiceCall.Tab) -> Void = { _ in }
    var onMute: () -> Void = {}
    var onEnd: () -> Void = {}

    var body: some View {
        VStack(spacing: 12) {
            CallTabs(active: .chat, onSelect: onTab)
            VoiceCallPill(
                view: view, callee: callee, timer: timer, level: level, bands: [], animated: animated,
                onMute: onMute, onEnd: onEnd, docked: true
            )
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
    }
}

extension View {
    /// FEED-272: an agent chat on screen. It registers with the call while it
    /// shows (so the root knows whether the Chat tab has to open one, and
    /// leaving it on the Chat tab minimizes the call into the pill).
    func callChat(agentId: String?) -> some View {
        modifier(CallChatRegistration(agentId: agentId))
    }
}

private struct CallChatRegistration: ViewModifier {
    let agentId: String?
    @State private var visible = false
    @State private var registered: String?

    func body(content: Content) -> some View {
        content
            .onAppear {
                visible = true
                sync()
            }
            .onDisappear {
                visible = false
                sync()
            }
            .onChange(of: agentId) { _, _ in sync() }
    }

    /// The agent id ends up registered exactly while the chat shows (rows
    /// that load late register on arrival).
    private func sync() {
        let want = visible ? agentId : nil
        guard want != registered else { return }
        let left = registered
        registered = want
        if let want { VoiceCallClient.shared.chatShown(agentId: want, true) }
        if let left { VoiceCallClient.shared.chatShown(agentId: left, false) }
    }
}
