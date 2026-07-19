import ExpCore
import ExpUI
import SwiftUI

/// The Support tab (EXP-180): the team helpdesk inbox as its own bottom-bar
/// destination — a tab that exists only while the active team's synced
/// `helpdesk_enabled` flag is on (MainNavigator gates it). Owns the screen
/// chrome; the list itself (filter pills, rows, poll lifecycle) lives in
/// SupportInboxListContent.
struct SupportView: View {
    @Environment(TeamState.self) private var teamState

    var body: some View {
        ZStack {
            AppBackground()

            if let teamId = teamState.activeTeam?.id {
                SupportInboxListContent(teamId: teamId)
                    .padding(.top, 8)
            } else {
                Color.clear
            }
        }
        .navigationTitle("Support")
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
    }
}
