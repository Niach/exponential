import ExpUI
import SwiftUI

/// Linear-style floating bottom navigation: a glass pill with the top-level
/// destinations (Projects, Inbox — with an unread dot) plus a detached circular
/// compose button on the right. Overlaid via safeAreaInset so content scrolls
/// underneath it; MainNavigator hides it on detail screens.
struct MobileTabBar: View {
    let homeActive: Bool
    let inboxActive: Bool
    let unreadCount: Int
    let showsCompose: Bool
    let onHome: () -> Void
    let onInbox: () -> Void
    let onCompose: () -> Void

    var body: some View {
        HStack(spacing: 12) {
            HStack(spacing: 4) {
                tab(icon: "square.grid.2x2", label: "Projects", active: homeActive, action: onHome)
                tab(icon: "tray", label: "Inbox", active: inboxActive, badge: unreadCount > 0, action: onInbox)
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
        icon: String,
        label: String,
        active: Bool,
        badge: Bool = false,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            Image(systemName: icon)
                .font(.body.weight(.medium))
                .foregroundStyle(.white.opacity(active ? 1 : TextOpacity.secondary))
                .frame(width: 56, height: 42)
                .overlay(alignment: .topTrailing) {
                    if badge {
                        Circle()
                            .fill(Accent.indigo)
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
}
