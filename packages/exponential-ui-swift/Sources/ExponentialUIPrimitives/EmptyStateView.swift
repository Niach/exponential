import SwiftUI

/// The empty state every client draws: an icon disc, a title, a description
/// and one action (the catalog `EmptyState` macro's parts; new on iOS,
/// SLOP-18 GAP-N).
public struct EmptyStateView<Icon: View>: View {
    let title: String
    let description: String?
    let actionLabel: String?
    let action: (() -> Void)?
    let icon: Icon

    public init(title: String, description: String? = nil, actionLabel: String? = nil, action: (() -> Void)? = nil, @ViewBuilder icon: () -> Icon) {
        self.title = title
        self.description = description
        self.actionLabel = actionLabel
        self.action = action
        self.icon = icon()
    }

    @Environment(\.primitiveTokens) private var tokens

    public var body: some View {
        VStack(spacing: 8) {
            if Icon.self != EmptyView.self {
                icon
                    .frame(width: 40, height: 40)
                    .background(tokens.muted, in: Circle())
                    .foregroundStyle(tokens.mutedForeground)
            }
            Text(title).font(.system(size: 14, weight: .medium)).foregroundStyle(tokens.foreground)
            if let description {
                Text(description).font(.system(size: 13)).foregroundStyle(tokens.mutedForeground).multilineTextAlignment(.center)
            }
            if let actionLabel, let action {
                Button(actionLabel, action: action)
                    .buttonStyle(.plain)
                    .font(.system(size: 13, weight: .medium))
                    .padding(.horizontal, 12)
                    .frame(height: tokens.pillHeight + 8)
                    .background(tokens.card, in: Capsule())
                    .overlay(Capsule().strokeBorder(tokens.border, lineWidth: tokens.hairline))
                    .foregroundStyle(tokens.foreground)
                    .padding(.top, 4)
            }
        }
        .frame(maxWidth: .infinity)
        .padding(24)
    }
}

extension EmptyStateView where Icon == EmptyView {
    public init(title: String, description: String? = nil, actionLabel: String? = nil, action: (() -> Void)? = nil) {
        self.init(title: title, description: description, actionLabel: actionLabel, action: action, icon: { EmptyView() })
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
