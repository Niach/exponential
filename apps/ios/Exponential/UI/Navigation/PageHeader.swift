import ExpCore
import ExpUI
import SwiftUI

/// #EXP-1384 (chats.json `shell.header`): THE PAGE HEADER on every top-level
/// phone page (`header.pages`: Board, Chats, Apps, Team, Devices, Agent) —
/// the large inline navigation title on the left and `header.buttons` as the
/// NATIVE toolbar circles on the right (plain ToolbarItems: the system's
/// iOS 26 Liquid Glass circles, never hand-drawn ones): SEARCH (the search
/// screen) then the INBOX (the inbox page; a sky dot while any notification
/// of the active team is unread — MainNavigator publishes `\.inboxUnread`).
/// A pushed page (Devices, `pushed: true`) gets the system back control
/// before its title: the system centres an inline title on a pushed page, so
/// there the large title rides a leading item right after the back control.
extension View {
    func pageHeader(_ title: String, pushed: Bool = false) -> some View {
        modifier(PageHeaderModifier(title: title, pushed: pushed))
    }
}

extension EnvironmentValues {
    /// The page header's inbox dot: an unread notification of the ACTIVE team.
    @Entry var inboxUnread = false
}

private struct PageHeaderModifier: ViewModifier {
    let title: String
    let pushed: Bool
    @Environment(\.inboxUnread) private var inboxUnread

    func body(content: Content) -> some View {
        let titled = content
            .navigationTitle(title)
            .toolbarTitleDisplayMode(pushed ? .inline : .inlineLarge)
            .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
            .modifier(PushedTitle(title: title, enabled: pushed))
        if #available(iOS 26.0, *) {
            // Two circles, not one shared capsule: a fixed spacer between them.
            titled.toolbar {
                ToolbarItem(placement: .topBarTrailing) { PageHeaderSearchButton() }
                ToolbarSpacer(.fixed, placement: .topBarTrailing)
                ToolbarItem(placement: .topBarTrailing) { PageHeaderInboxButton(dot: inboxUnread) }
            }
        } else {
            titled.toolbar {
                ToolbarItemGroup(placement: .topBarTrailing) {
                    PageHeaderSearchButton()
                    PageHeaderInboxButton(dot: inboxUnread)
                }
            }
        }
    }
}

/// The header's search entry — the same bare 32pt glyph the system toolbar
/// glass wraps into its circle.
struct PageHeaderSearchButton: View {
    var body: some View {
        NavigationLink(value: AppRoute.search) {
            PageHeaderGlyph(icon: AppIcons.navSearch, dot: false)
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Search")
        .accessibilityIdentifier(Chats.TestIds.searchButton)
    }
}

/// The header's inbox entry, its sky dot while the active team has news.
struct PageHeaderInboxButton: View {
    let dot: Bool

    var body: some View {
        NavigationLink(value: AppRoute.myWork) {
            PageHeaderGlyph(icon: AppIcons.navInbox, dot: dot)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(dot ? "\(Chats.Copy.inbox), unread" : Chats.Copy.inbox)
        .accessibilityIdentifier(Chats.TestIds.inboxButton)
    }
}

/// One header glyph (the system draws the circle around it on iOS 26).
struct PageHeaderGlyph: View {
    let icon: String
    let dot: Bool

    var body: some View {
        AppIcon(icon, size: AppIcon.Size.medium)
            .foregroundStyle(.white)
            .frame(width: 32, height: 32)
            .overlay(alignment: .topTrailing) {
                if dot {
                    Circle()
                        .fill(DesignTokens.Semantic.blue)
                        .frame(width: 8, height: 8)
                        .offset(x: 1, y: 1)
                }
            }
            .contentShape(Circle())
    }
}

/// A pushed page's large title as the leading item after the back control
/// (the principal inline title stands down).
private struct PushedTitle: ViewModifier {
    let title: String
    let enabled: Bool

    func body(content: Content) -> some View {
        if enabled {
            content
                .toolbar {
                    ToolbarItem(placement: .principal) {
                        Color.clear.frame(width: 1, height: 1).accessibilityHidden(true)
                    }
                }
                .ownChromeToolbarItem(placement: .topBarLeading) {
                    Text(title)
                        .font(.system(size: 34, weight: .bold))
                        .foregroundStyle(.white)
                        .lineLimit(1)
                        .fixedSize()
                        .accessibilityAddTraits(.isHeader)
                }
        } else {
            content
        }
    }
}
