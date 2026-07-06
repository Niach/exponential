import ExpUI
import SwiftUI

/// Linear-style floating bottom navigation: a glass pill with the four
/// top-level destinations (Issues, Search, Agents — with a running-session
/// dot — and Inbox — with an unread dot) plus a detached circular compose
/// button on the right. Overlaid via safeAreaInset so content scrolls
/// underneath it; MainNavigator hides it on detail screens.
struct MobileTabBar: View {
    let issuesActive: Bool
    let searchActive: Bool
    let agentsActive: Bool
    let inboxActive: Bool
    let unreadCount: Int
    let agentsRunning: Bool
    let showsCompose: Bool
    let onIssues: () -> Void
    let onSearch: () -> Void
    let onAgents: () -> Void
    let onInbox: () -> Void
    let onCompose: () -> Void

    /// SF Symbols has no robot-head glyph, so the Agents tab draws a bundled
    /// template vector asset; every other tab keeps a system symbol.
    private enum TabGlyph {
        case system(String)
        case asset(String)
    }

    var body: some View {
        HStack(spacing: 12) {
            HStack(spacing: 4) {
                tab(glyph: .system("list.bullet"), label: "Issues", active: issuesActive, action: onIssues)
                tab(glyph: .system("magnifyingglass"), label: "Search", active: searchActive, action: onSearch)
                tab(
                    glyph: .asset("tab-robot"),
                    label: "Agents",
                    active: agentsActive,
                    badge: agentsRunning,
                    badgeColor: DesignTokens.Semantic.green,
                    action: onAgents
                )
                tab(
                    glyph: .system("tray"),
                    label: "Inbox",
                    active: inboxActive,
                    badge: unreadCount > 0,
                    badgeColor: Accent.indigo,
                    action: onInbox
                )
            }
            .padding(5)
            .background(.ultraThinMaterial, in: Capsule())
            .overlay(
                Capsule().stroke(Color.white.opacity(0.12), lineWidth: 0.5)
            )
            .shadow(color: .black.opacity(0.35), radius: 16, y: 6)

            Spacer()

            if showsCompose {
                Button(action: onCompose) {
                    Image(systemName: "square.and.pencil")
                        .font(.body.weight(.semibold))
                        .foregroundStyle(.white)
                        .frame(width: 52, height: 52)
                        .background(.ultraThinMaterial, in: Circle())
                        .overlay(
                            Circle().stroke(Color.white.opacity(0.12), lineWidth: 0.5)
                        )
                        .shadow(color: .black.opacity(0.35), radius: 16, y: 6)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("New issue")
            }
        }
        .padding(.horizontal, 20)
        .padding(.top, 8)
        .padding(.bottom, 4)
    }

    private func tab(
        glyph: TabGlyph,
        label: String,
        active: Bool,
        badge: Bool = false,
        badgeColor: Color = Accent.indigo,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            glyphImage(glyph, active: active)
                .frame(width: 56, height: 42)
                .overlay(alignment: .topTrailing) {
                    if badge {
                        Circle()
                            .fill(badgeColor)
                            .frame(width: 8, height: 8)
                            .offset(x: -14, y: 8)
                    }
                }
                .background(active ? Color.white.opacity(0.12) : .clear, in: Capsule())
                .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
    }

    @ViewBuilder
    private func glyphImage(_ glyph: TabGlyph, active: Bool) -> some View {
        switch glyph {
        case let .system(name):
            Image(systemName: name)
                .font(.body.weight(.medium))
                .foregroundStyle(.white.opacity(active ? 1 : TextOpacity.secondary))
        case let .asset(name):
            Image(name)
                .renderingMode(.template)
                .resizable()
                .scaledToFit()
                .frame(width: 20, height: 20)
                .foregroundStyle(.white.opacity(active ? 1 : TextOpacity.secondary))
        }
    }
}
