import ExpCore
import ExpUI
import SwiftUI

/// #EXP-2109 (chats.json `shell.header`): THE PAGE HEADER. A root page
/// (`header.pages`: Chats, Teams, Settings) wears the large inline navigation
/// title on the left and `header.buttons` on the right, each THE round header
/// button (#EXP-2168 header-actions.json: `HeaderCircleFace` in a
/// `HeaderToolbarItem`, one look on every iOS): SEARCH (the search screen)
/// then the INBOX (the inbox page; #FEED-232 THE count badge, muted, of every
/// team's unread notifications, `\.inboxUnreadCount`). A pushed page (team,
/// folder, board, a settings page, Devices; `pushed: true`) = the phone
/// DETAIL header: THE round back, the leading glyph, the title, all leading
/// (EXP-1554: the system centres an inline title on a pushed page, so it
/// rides the back's item; a custom title = `detailHeaderTitle`).
extension View {
    func pageHeader(_ title: String, pushed: Bool = false, glyph: PageHeaderGlyphKind? = nil) -> some View {
        modifier(PageHeaderModifier(title: title, pushed: pushed, glyph: glyph) { EmptyView() } more: { EmptyView() })
    }

    /// A root page with its own circle after the search and inbox ones
    /// (header-actions.json).
    func pageHeader<Action: View>(_ title: String, @ViewBuilder action: () -> Action) -> some View {
        modifier(PageHeaderModifier(title: title, pushed: false, glyph: nil, action: action) { EmptyView() })
    }

    /// A pushed page with its + and `…` (header-actions.json; after search
    /// and inbox while it is an iPad column's root).
    func pageHeader<Add: View, More: View>(
        _ title: String, pushed: Bool, glyph: PageHeaderGlyphKind?,
        @ViewBuilder add: () -> Add, @ViewBuilder more: () -> More
    ) -> some View {
        modifier(PageHeaderModifier(title: title, pushed: pushed, glyph: glyph, action: add, more: more))
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

private struct PageHeaderModifier<Action: View, More: View>: ViewModifier {
    let title: String
    let requestedPushed: Bool
    let glyph: PageHeaderGlyphKind?
    @Environment(\.inboxUnreadCount) private var inboxUnreadCount
    @Environment(\.pageHeaderAsRoot) private var asRoot

    let action: Action
    let more: More

    init(
        title: String, pushed: Bool, glyph: PageHeaderGlyphKind?, @ViewBuilder action: () -> Action,
        @ViewBuilder more: () -> More
    ) {
        self.title = title
        requestedPushed = pushed
        self.glyph = glyph
        self.action = action()
        self.more = more()
    }

    private var pushed: Bool { requestedPushed && !asRoot }

    func body(content: Content) -> some View {
        let titled = content
            .navigationTitle(title)
            .toolbarTitleDisplayMode(pushed ? .inline : .inlineLarge)
            .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
            .modifier(PushedTitle(title: PushedTitleKey.Title(text: title, glyph: glyph), enabled: pushed))
        if #available(iOS 26.0, *) {
            // Circles, not one shared capsule: a fixed spacer between them.
            titled.toolbar {
                if !pushed {
                    HeaderToolbarItem { PageHeaderSearchButton() }
                    ToolbarSpacer(.fixed, placement: .topBarTrailing)
                    HeaderToolbarItem { PageHeaderInboxButton(count: inboxUnreadCount) }
                    ToolbarSpacer(.fixed, placement: .topBarTrailing)
                }
                if Action.self != EmptyView.self {
                    HeaderToolbarItem { action }
                    ToolbarSpacer(.fixed, placement: .topBarTrailing)
                }
                if More.self != EmptyView.self {
                    HeaderToolbarItem { more }
                }
            }
        } else {
            titled.toolbar {
                ToolbarItemGroup(placement: .topBarTrailing) {
                    HStack(spacing: CGFloat(HeaderActions.gap)) {
                        if !pushed {
                            PageHeaderSearchButton()
                            PageHeaderInboxButton(count: inboxUnreadCount)
                        }
                        action
                        more
                    }
                }
            }
        }
    }
}

/// The header's search entry, THE round header button.
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

/// A header's own trailing circle (header-actions.json), in a toolbar.
struct PageHeaderButton: View {
    let icon: String
    let label: String
    var identifier = ""
    let action: () -> Void

    var body: some View {
        Button(action: action) { PageHeaderGlyph(icon: icon) }
            .buttonStyle(.plain)
            .accessibilityLabel(label)
            .accessibilityIdentifier(identifier)
    }
}

/// THE round header button's face in a navigation bar (header-actions.json
/// `circle` / `glyph`: `HeaderCircleFace`, the custom bars' twin), its count
/// badge over the circle. Host it in a `HeaderToolbarItem`: the system's own
/// item glass stands down, so iOS 26 and earlier draw the one circle.
struct PageHeaderGlyph: View {
    let icon: String
    var count = 0

    var body: some View {
        HeaderCircleFace(icon: icon)
            .overlay(alignment: .topTrailing) {
                CountBadge(count: count, tone: .muted)
                    .offset(x: 2, y: 0)
            }
    }
}

/// #EXP-2168: a navigation bar item holding THE round header button: the
/// item's shared Liquid Glass stands down (the face draws its own circle),
/// never two concentric rings.
struct HeaderToolbarItem<Content: View>: ToolbarContent {
    var placement: ToolbarItemPlacement = .topBarTrailing
    @ViewBuilder let content: () -> Content

    var body: some ToolbarContent {
        if #available(iOS 26.0, *) {
            ToolbarItem(placement: placement) { content() }
                .sharedBackgroundVisibility(.hidden)
        } else {
            ToolbarItem(placement: placement) { content() }
        }
    }
}

extension View {
    /// #EXP-2168: a pushed page's OWN title view (an issue's collapsing
    /// title, an app's glyph and name), LEFT after THE round back (it rides
    /// the back's item like `pageHeader(pushed: true)`); `key` names what it
    /// shows (the item redraws when it changes). An iPad detail column's root
    /// (no back) wears it as its own leading item.
    func detailHeaderTitle<Title: View>(_ key: String, @ViewBuilder title: () -> Title) -> some View {
        modifier(DetailHeaderTitle(key: key, title: title()))
    }
}

private struct DetailHeaderTitle<Title: View>: ViewModifier {
    let key: String
    let title: Title
    @Environment(\.detailColumnRoot) private var columnRoot

    func body(content: Content) -> some View {
        if columnRoot {
            content.ownChromeToolbarItem(placement: .topBarLeading) {
                title
                    .frame(minWidth: 0, idealWidth: PageHeaderDetailTitle.customWidth, maxWidth: PageHeaderDetailTitle.customWidth, alignment: .leading)
                    .fixedSize(horizontal: true, vertical: false)
            }
        } else {
            content.modifier(PushedTitle(
                title: PushedTitleKey.Title(text: key, custom: .init(key: key, view: AnyView(title))),
                enabled: true
            ))
        }
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
    /// A custom title's width (the phone's bar between THE round back and
    /// one trailing circle).
    static let customWidth: CGFloat = 240

    var body: some View {
        if let custom = title.custom {
            // A bar item proposes no width to a flexible title: it takes
            // its own, capped so a long one truncates.
            custom.view
                .frame(minWidth: 0, idealWidth: Self.customWidth, maxWidth: Self.customWidth, alignment: .leading)
                .fixedSize(horizontal: true, vertical: false)
        } else {
            plain
        }
    }

    private var plain: some View {
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
