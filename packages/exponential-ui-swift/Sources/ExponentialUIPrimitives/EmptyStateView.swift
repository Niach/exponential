import SwiftUI

/// EXP-1274: the two empty-state sizes (`packages/domain-contract/fixtures/
/// empty-state.json` `sizes`, the catalog `EmptyState` macro's `size`).
/// `page` = what a whole screen says when it has nothing: the icon disc, a
/// title, an optional sentence and an optional actions row. `list` = what a
/// list inside a section, a sidebar or a picker says: a bare muted glyph over
/// ONE muted line, never a description or a button.
public enum EmptyStateSize: String, Sendable, CaseIterable {
    case page, list

    /// The disc diameter (`page` only; `list` draws the bare glyph).
    public var disc: CGFloat? { self == .page ? 48 : nil }
    /// The glyph's point size.
    public var glyph: CGFloat { self == .page ? 24 : 20 }
    public var paddingY: CGFloat { self == .page ? 48 : 24 }
    /// The column's maximum width (`page` only).
    public var maxWidth: CGFloat? { self == .page ? 448 : nil }
    /// The vertical gap between the parts.
    public var gap: CGFloat { self == .page ? 12 : 6 }
}

/// The fonts an empty state sets its text in. The default is fixed-size
/// (the painter's); an embedding app passes its Dynamic Type styles.
public struct EmptyStateFonts {
    public var title: Font
    public var description: Font
    public var listTitle: Font

    public init(
        title: Font = .system(size: 17, weight: .semibold),
        description: Font = .system(size: 14),
        listTitle: Font = .system(size: 14)
    ) {
        self.title = title
        self.description = description
        self.listTitle = listTitle
    }

    public static let standard = EmptyStateFonts()
}

/// The empty state every client draws (the catalog `EmptyState` macro's
/// parts; SLOP-18 GAP-N): `page` = an icon disc, a title, a description and
/// an actions row (one labelled button, or any view the host builds);
/// `list` = a bare glyph over one muted line (EXP-1274).
public struct EmptyStateView<Icon: View, Actions: View>: View {
    let title: String
    let description: String?
    let size: EmptyStateSize
    let fonts: EmptyStateFonts
    let icon: Icon
    let actions: Actions
    /// Whether the actions row draws (a single-button slot with no label or
    /// no action has nothing to draw, so it takes no gap either).
    let hasActions: Bool

    /// Any actions row (the host's own buttons); ignored on `list`.
    public init(
        title: String,
        description: String? = nil,
        size: EmptyStateSize = .page,
        fonts: EmptyStateFonts = .standard,
        @ViewBuilder icon: () -> Icon,
        @ViewBuilder actions: () -> Actions
    ) {
        self.title = title
        self.description = description
        self.size = size
        self.fonts = fonts
        self.icon = icon()
        self.actions = actions()
        self.hasActions = Actions.self != EmptyView.self
    }

    init(title: String, description: String?, size: EmptyStateSize, fonts: EmptyStateFonts, icon: Icon, actions: Actions, hasActions: Bool) {
        self.title = title
        self.description = description
        self.size = size
        self.fonts = fonts
        self.icon = icon
        self.actions = actions
        self.hasActions = hasActions
    }

    @Environment(\.primitiveTokens) private var tokens

    public var body: some View {
        switch size {
        case .page: page
        case .list: list
        }
    }

    private var page: some View {
        VStack(spacing: size.gap) {
            if Icon.self != EmptyView.self {
                icon
                    .frame(width: size.disc, height: size.disc)
                    .background(tokens.muted, in: Circle())
                    .foregroundStyle(tokens.mutedForeground)
            }
            Text(title)
                .font(fonts.title)
                .foregroundStyle(tokens.foreground)
                .multilineTextAlignment(.center)
            if let description {
                Text(description)
                    .font(fonts.description)
                    .foregroundStyle(tokens.mutedForeground)
                    .multilineTextAlignment(.center)
            }
            if hasActions {
                actions.padding(.top, 4)
            }
        }
        .padding(.horizontal, 24)
        .padding(.vertical, size.paddingY)
        .frame(maxWidth: size.maxWidth)
        .frame(maxWidth: .infinity)
        .accessibilityElement(children: .contain)
    }

    private var list: some View {
        VStack(spacing: size.gap) {
            if Icon.self != EmptyView.self {
                icon.foregroundStyle(tokens.mutedForeground.opacity(0.6))
                    .accessibilityHidden(true)
            }
            Text(title)
                .font(fonts.listTitle)
                .foregroundStyle(tokens.mutedForeground)
                .multilineTextAlignment(.center)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, size.paddingY)
        .frame(maxWidth: .infinity)
    }
}

/// The labelled button the catalog's `EmptyState` action paints.
public struct EmptyStateButton: View {
    let label: String
    let action: () -> Void

    public init(_ label: String, action: @escaping () -> Void) {
        self.label = label
        self.action = action
    }

    @Environment(\.primitiveTokens) private var tokens

    public var body: some View {
        Button(label, action: action)
            .buttonStyle(.plain)
            .font(.system(size: 13, weight: .medium))
            .padding(.horizontal, 12)
            .frame(height: tokens.pillHeight + 8)
            .background(tokens.card, in: Capsule())
            .overlay(Capsule().strokeBorder(tokens.border, lineWidth: tokens.hairline))
            .foregroundStyle(tokens.foreground)
    }
}

extension EmptyStateView where Actions == EmptyStateButtonSlot {
    /// One labelled action (shown only when both `actionLabel` and `action`
    /// are given, and only on `page`).
    public init(
        title: String,
        description: String? = nil,
        size: EmptyStateSize = .page,
        fonts: EmptyStateFonts = .standard,
        actionLabel: String? = nil,
        action: (() -> Void)? = nil,
        @ViewBuilder icon: () -> Icon
    ) {
        self.init(
            title: title,
            description: description,
            size: size,
            fonts: fonts,
            icon: icon(),
            actions: EmptyStateButtonSlot(label: actionLabel, action: action),
            hasActions: actionLabel != nil && action != nil
        )
    }
}

extension EmptyStateView where Icon == EmptyView, Actions == EmptyStateButtonSlot {
    public init(
        title: String,
        description: String? = nil,
        size: EmptyStateSize = .page,
        fonts: EmptyStateFonts = .standard,
        actionLabel: String? = nil,
        action: (() -> Void)? = nil
    ) {
        self.init(title: title, description: description, size: size, fonts: fonts, actionLabel: actionLabel, action: action, icon: { EmptyView() })
    }
}

/// The optional single-button actions row (`EmptyStateButton` when both
/// parts are given, else nothing).
public struct EmptyStateButtonSlot: View {
    let label: String?
    let action: (() -> Void)?

    public var body: some View {
        if let label, let action {
            EmptyStateButton(label, action: action)
        }
    }
}

/// A disclosure header: a chevron that turns, a title and an optional
/// count, toggling `isExpanded` (the catalog `Collapsible` / `Accordion`
/// triggers; new on iOS, SLOP-18 GAP-N).
public struct DisclosureHeader: View {
    let title: String
    let count: Int?
    @Binding var isExpanded: Bool

    public init(_ title: String, count: Int? = nil, isExpanded: Binding<Bool>) {
        self.title = title
        self.count = count
        self._isExpanded = isExpanded
    }

    @Environment(\.primitiveTokens) private var tokens

    public var body: some View {
        Button {
            withAnimation(.easeInOut(duration: 0.15)) { isExpanded.toggle() }
        } label: {
            HStack(spacing: 8) {
                Text(title).font(.system(size: 14, weight: .medium)).foregroundStyle(tokens.foreground).lineLimit(1)
                if let count {
                    Text("\(count)").font(.system(size: 12)).foregroundStyle(tokens.mutedForeground)
                }
                Spacer(minLength: 0)
                Image(systemName: "chevron.down")
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(tokens.mutedForeground)
                    .rotationEffect(.degrees(isExpanded ? 180 : 0))
            }
            .frame(height: tokens.rowHeight)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(.isHeader)
        .accessibilityValue(isExpanded ? "expanded" : "collapsed")
    }
}
