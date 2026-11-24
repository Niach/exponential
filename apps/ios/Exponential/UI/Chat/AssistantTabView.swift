import ExpCore
import ExpUI
import SwiftUI

/// #EXP-2109 (chats.json `shell.tabs`): THE ASSISTANT TAB: the viewer's one
/// Assistant conversation, its chat screen without a back control, the
/// composer above the bar (the bar hides while it has the keyboard).
struct AssistantTabView: View {
    /// The page of the chat it opens on (a push, a link); nil = its Conversation.
    var page: ChatPage?

    @Environment(AppDependencies.self) private var deps
    @Environment(SessionListsModel.self) private var lists
    @Environment(\.accountId) private var accountId
    /// The server was asked for the Assistant (its row syncs in after).
    @State private var asked = false
    @State private var answered: String?

    var body: some View {
        Group {
            if let agentId = lists.assistant?.agentId ?? answered {
                AgentChatRoute(agentId: agentId, page: page)
                    .id(agentId)
            } else {
                ZStack {
                    AppBackground()
                    if lists.loaded, asked {
                        EmptyState(title: AgentSettings.AssistantCopy.noDevice, icon: AppIcons.uiDevice)
                            .frame(maxWidth: .infinity, maxHeight: .infinity)
                    } else {
                        SkeletonRows()
                            .padding(.horizontal, 16)
                            .padding(.top, 8)
                            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
                    }
                }
                .toolbar(.hidden, for: .navigationBar)
            }
        }
        .task(id: accountId) {
            answered = await AssistantEnsure.run(deps, accountId: accountId, teamId: nil)
            asked = true
        }
        .accessibilityIdentifier("assistant-tab")
    }
}
