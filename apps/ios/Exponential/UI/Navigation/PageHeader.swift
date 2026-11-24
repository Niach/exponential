import ExpCore
import ExpUI
import SwiftUI

/// #EXP-2109 (chats.json `shell.header`): THE PAGE HEADER. A root page
/// (`header.pages`: Chats, Teams, Settings) wears the large inline navigation
/// title on the left and `header.buttons` as the NATIVE toolbar circles on
/// the right (plain ToolbarItems: the system's iOS 26 Liquid Glass circles,
/// never hand-drawn ones): SEARCH (the search screen) then the INBOX (the
/// inbox page; #FEED-232 THE count badge, muted, of every team's unread
/// notifications, `\.inboxUnreadCount`). A pushed page (team, folder, board,
/// a settings page, Devices; `pushed: true`) = the phone DETAIL header: the
/// bare back arrow, the leading glyph, the title, all leading (EXP-1554: the
/// system centres an inline title on a pushed page, so it rides the back
/// arrow's item).
extension View {
    func pageHeader(_ title: String, pushed: Bool = false, glyph: PageHeaderGlyphKind? = nil) -> some View {
        modifier(PageHeaderModifier(title: title, pushed: pushed, glyph: glyph))
    }
}

/// A detail header's leading glyph.
enum PageHeaderGlyphKind: Equatable {
    /// The team's mark.
    case team(iconUrl: String?)
    case folder
    /// A board's (agent's, app's) glyph in its colour.
    case icon(String, color: String)
}

extension EnvironmentValues {
    /// #FEED-232: the page header's Inbox count: every team's unread
    /// notifications.
    @Entry var inboxUnreadCount = 0
    /// #NEW-12: the iPad's split columns host a `pushed` page (Needs you,
    /// Devices, a board) as a column ROOT: no back arrow to carry its title.
    @Entry var pageHeaderAsRoot = false
    /// #NEW-12: the page is the iPad detail column's ROOT (a list row opened
    /// it beside its list): there is nothing to go back to. #EXP-2109: the
    /// phone's Assistant TAB too.
    @Entry var detailColumnRoot = false
}

private struct PageHeaderModifier: ViewModifier {
    let title: String
    let requestedPushed: Bool
    let glyph: PageHeaderGlyphKind?
    @Environment(\.inboxUnreadCount) private var inboxUnreadCount
    @Environment(\.pageHeaderAsRoot) private var asRoot

    init(title: String, pushed: Bool, glyph: PageHeaderGlyphKind?) {
        self.title = title
        requestedPushed = pushed
        self.glyph = glyph
    }

    private var pushed: Bool { requestedPushed && !asRoot }

    func body(content: Content) -> some View {
        let titled = content
            .navigationTitle(title)
            .toolbarTitleDisplayMode(pushed ? .inline : .inlineLarge)
            .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
            .modifier(PushedTitle(title: PushedTitleKey.Title(text: title, glyph: glyph), enabled: pushed))
        if pushed {
            titled
        } else if #available(iOS 26.0, *) {
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

/// The header's inbox entry, its count badge over every team.
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

/// A pushed page's title, handed up to the bare back item
/// (`PushedTitleKey`; the principal inline title stands down).
private struct PushedTitle: ViewModifier {
    let title: PushedTitleKey.Title
    let enabled: Bool

    func body(content: Content) -> some View {
        if enabled {
            content
                .toolbar {
                    ToolbarItem(placement: .principal) {
                        Color.clear.frame(width: 1, height: 1).accessibilityHidden(true)
                    }
                }
                .preference(key: PushedTitleKey.self, value: title)
        } else {
            content
        }
    }
}

/// The detail header's leading part after the back arrow.
struct PageHeaderDetailTitle: View {
    let title: PushedTitleKey.Title

    var body: some View {
        HStack(spacing: 10) {
            if let glyph = title.glyph {
                Group {
                    switch glyph {
                    case let .team(iconUrl):
                        TeamAvatar(iconUrl: iconUrl, size: 24).foregroundStyle(.white)
                    case .folder:
                        AppIcon(AppIcons.uiFolder, size: 20)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    case let .icon(name, color):
                        AppIcon(name, size: 20).foregroundStyle(Color(hex: color) ?? .gray)
                    }
                }
                .frame(width: 24, height: 24)
            }
            Text(title.text)
                .font(.system(size: 17, weight: .semibold))
                .foregroundStyle(.white)
                .lineLimit(1)
                .fixedSize()
                .accessibilityAddTraits(.isHeader)
        }
    }
}
