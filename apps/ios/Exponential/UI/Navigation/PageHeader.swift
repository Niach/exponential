import ExpCore
import ExpUI
import SwiftUI

/// #EXP-1384 (chats.json `shell.header`): THE PAGE HEADER on every top-level
/// phone page (`header.pages`: Board, Chats, Apps, Team, Devices, Agent) —
/// the large inline navigation title on the left and `header.buttons` as the
/// NATIVE toolbar circles on the right (plain ToolbarItems: the system's
/// iOS 26 Liquid Glass circles, never hand-drawn ones): SEARCH (the search
/// screen) then the INBOX (the inbox page; #FEED-232 THE count badge, muted,
/// of the active team's unread notifications — MainNavigator publishes
/// `\.inboxUnreadCount`).
/// A pushed page (Devices, `pushed: true`) gets the bare back arrow before
/// its title (EXP-1554, `bareBackButton`): the system centres an inline
/// title on a pushed page, so there the large title rides the back arrow's
/// leading item.
extension View {
    func pageHeader(_ title: String, pushed: Bool = false) -> some View {
        modifier(PageHeaderModifier(title: title, pushed: pushed))
    }
}

extension EnvironmentValues {
    /// #FEED-232: the page header's Inbox count: the ACTIVE team's unread
    /// notifications.
    @Entry var inboxUnreadCount = 0
    /// #NEW-12: the iPad's split columns host a `pushed` page (Needs you,
    /// Devices) as a column ROOT: no back arrow to carry its title.
    @Entry var pageHeaderAsRoot = false
    /// #NEW-12: the page is the iPad detail column's ROOT (a list row opened
    /// it beside its list): there is nothing to go back to.
    @Entry var detailColumnRoot = false
}

private struct PageHeaderModifier: ViewModifier {
    let title: String
    let requestedPushed: Bool
    @Environment(\.inboxUnreadCount) private var inboxUnreadCount
    @Environment(\.pageHeaderAsRoot) private var asRoot

    init(title: String, pushed: Bool) {
        self.title = title
        requestedPushed = pushed
    }

    private var pushed: Bool { requestedPushed && !asRoot }

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
                ToolbarItem(placement: .topBarTrailing) { PageHeaderInboxButton(count: inboxUnreadCount) }
            }
        } else {
            titled.toolbar {
                ToolbarItemGroup(placement: .topBarTrailing) {
                    PageHeaderSearchButton()
                    PageHeaderInboxButton(count: inboxUnreadCount)
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
            PageHeaderGlyph(icon: AppIcons.navSearch)
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Search")
        .accessibilityIdentifier(Chats.TestIds.searchButton)
    }
}

/// The header's inbox entry, its count badge while the active team has news.
struct PageHeaderInboxButton: View {
    let count: Int

    var body: some View {
        NavigationLink(value: AppRoute.myWork) {
            PageHeaderGlyph(icon: AppIcons.navInbox, count: count)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(count > 0 ? "\(Chats.Copy.inbox), \(count) unread" : Chats.Copy.inbox)
        .accessibilityIdentifier(Chats.TestIds.inboxButton)
    }
}

/// One header glyph (the system draws the circle around it on iOS 26).
struct PageHeaderGlyph: View {
    let icon: String
    var count = 0

    var body: some View {
        AppIcon(icon, size: AppIcon.Size.medium)
            .foregroundStyle(.white)
            .frame(width: 32, height: 32)
            .overlay(alignment: .topTrailing) {
                CountBadge(count: count, tone: .muted)
                    .offset(x: 4, y: -2)
            }
            .contentShape(Circle())
    }
}

/// A pushed page's large title, handed up to the bare back item
/// (`PushedLargeTitleKey`; the principal inline title stands down).
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
                .preference(key: PushedLargeTitleKey.self, value: title)
        } else {
            content
        }
    }
}
