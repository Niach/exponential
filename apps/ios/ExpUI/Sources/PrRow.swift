import ExpCore
import SwiftUI

// EXP-1248: THE pull-request row, ×4 (web `@exp/ui` `pr-row.tsx`, desktop
// `pr_rows.rs`, Android `PrRow.kt`; geometry = `list-item.json`): ONE line,
// [ring lead · mono identifier · title · quiet word]. No branch line, no PR
// number column, no counts, no age, no inline Merge. Reviews, PR trees and
// the Guide's stack card all draw it. Two shapes: a TREE nests with tree
// guides (`PrList`), a linear STACK never nests, it hangs off one rail down
// to its base branch (`StackRail`).

/// The ring lead: an emerald ring (open PR), a filled centre (the current
/// stack member), a muted ring (the base branch). A 14pt box like the run
/// mark, so tree guides land on its centre.
public struct PrNode: View {
    private let state: PrNodeState

    public init(state: PrNodeState) {
        self.state = state
    }

    public var body: some View {
        let ink: Color = state.ring == .muted
            ? .white.opacity(TextOpacity.tertiary)
            : DesignTokens.Semantic.green
        ZStack {
            Circle()
                .strokeBorder(ink, lineWidth: 1.5)
                .frame(width: ListItem.prNode, height: ListItem.prNode)
            if state.filled {
                Circle()
                    .fill(ink)
                    .frame(width: 6, height: 6)
            }
        }
        .frame(width: ListItem.mark, height: ListItem.mark)
        .accessibilityHidden(true)
    }
}

/// Which rail segments a stack row draws: into the node from above, out of
/// it below. They stop at the ring, never cross it.
public struct PrRailSegments: Equatable, Sendable {
    public var above: Bool
    public var below: Bool

    public init(above: Bool = false, below: Bool = false) {
        self.above = above
        self.below = below
    }
}

/// One PR line. CONTENT only: the host wraps it in its own tap.
public struct PrRow<Trailing: View>: View {
    private let node: PrNodeState
    private let identifier: String?
    private let title: String
    private let word: String?
    private let guide: TreeGuide
    private let rail: PrRailSegments?
    private let active: Bool
    private let trailing: Trailing

    /// - Parameters:
    ///   - title: the PR's issue title, or the base branch's name on a
    ///     `base` row (mono, muted).
    ///   - word: a quiet trailing word (`stack` on a stack's top row).
    ///   - guide: tree shape — this row's connector (its depth indents).
    ///   - rail: stack shape — the rail through the node centres.
    public init(
        node: PrNodeState = .open,
        identifier: String? = nil,
        title: String,
        word: String? = nil,
        guide: TreeGuide = TreeGuide(),
        rail: PrRailSegments? = nil,
        active: Bool = false,
        @ViewBuilder trailing: () -> Trailing
    ) {
        self.node = node
        self.identifier = identifier
        self.title = title
        self.word = word
        self.guide = guide
        self.rail = rail
        self.active = active
        self.trailing = trailing()
    }

    public var body: some View {
        let base = node == .base
        HStack(alignment: .center, spacing: ListItem.gap) {
            PrNode(state: node)
            if let identifier, !identifier.isEmpty {
                Text(identifier)
                    .font(.caption.monospaced())
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(1)
                    .frame(minWidth: 72, alignment: .leading)
                    .fixedSize()
            }
            Text(title)
                .font(base ? .caption.monospaced() : .subheadline)
                .foregroundStyle(base ? .white.opacity(TextOpacity.tertiary) : .white)
                .lineLimit(1)
                .truncationMode(.tail)
            Spacer(minLength: 0)
            if let word, !word.isEmpty {
                Text(word)
                    .font(.system(size: 11.5))
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(1)
                    .fixedSize()
            }
            trailing
        }
        .padding(.leading, ListItem.leadX(depth: guide.depth))
        .padding(.trailing, ListItem.base)
        .frame(height: ListItem.prRowPhone)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(TreeGuidesOverlay(guide: guide, base: ListItem.base))
        .background(railOverlay)
        .contentShape(Rectangle())
        .flatRow(isActive: active)
        .accessibilityElement(children: .combine)
    }

    @ViewBuilder
    private var railOverlay: some View {
        if let rail, rail.above || rail.below {
            GeometryReader { proxy in
                let x = ListItem.leadX(depth: guide.depth) + ListItem.mark / 2
                let middle = proxy.size.height / 2
                let half = ListItem.prNode / 2
                Path { path in
                    if rail.above {
                        path.move(to: CGPoint(x: x, y: 0))
                        path.addLine(to: CGPoint(x: x, y: middle - half))
                    }
                    if rail.below {
                        path.move(to: CGPoint(x: x, y: middle + half))
                        path.addLine(to: CGPoint(x: x, y: proxy.size.height))
                    }
                }
                .stroke(GlassTokens.strokeStrong, lineWidth: ListItem.rail)
            }
            .allowsHitTesting(false)
            .accessibilityHidden(true)
        }
    }
}

extension PrRow where Trailing == EmptyView {
    public init(
        node: PrNodeState = .open,
        identifier: String? = nil,
        title: String,
        word: String? = nil,
        guide: TreeGuide = TreeGuide(),
        rail: PrRailSegments? = nil,
        active: Bool = false
    ) {
        self.init(
            node: node, identifier: identifier, title: title, word: word,
            guide: guide, rail: rail, active: active, trailing: { EmptyView() }
        )
    }
}

/// One row of a PR tree: `depth` nests it under its parent.
public struct PrListRow: Identifiable {
    public let id: String
    public let identifier: String?
    public let title: String
    public let depth: Int
    public let node: PrNodeState
    public let word: String?
    public let active: Bool
    /// A muted external-link glyph at the right (an unlinked PR opens GitHub).
    public let external: Bool
    public let onOpen: (() -> Void)?

    public init(
        id: String,
        identifier: String? = nil,
        title: String,
        depth: Int = 0,
        node: PrNodeState = .open,
        word: String? = nil,
        active: Bool = false,
        external: Bool = false,
        onOpen: (() -> Void)? = nil
    ) {
        self.id = id
        self.identifier = identifier
        self.title = title
        self.depth = depth
        self.node = node
        self.word = word
        self.active = active
        self.external = external
        self.onOpen = onOpen
    }
}

/// A PR TREE (or a flat run of single PRs): rows nest with tree guides,
/// gapless (nothing for a connector to bridge).
public struct PrList: View {
    private let rows: [PrListRow]
    private let rowIdentifier: String

    public init(rows: [PrListRow], rowIdentifier: String = "pr-row") {
        self.rows = rows
        self.rowIdentifier = rowIdentifier
    }

    public var body: some View {
        let guides = TreeGuides.compute(depths: rows.map(\.depth))
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(rows.enumerated()), id: \.element.id) { index, row in
                PrTapTarget(onOpen: row.onOpen) {
                    PrRow(
                        node: row.node, identifier: row.identifier, title: row.title,
                        word: row.word, guide: guides[index], active: row.active
                    ) {
                        if row.external {
                            AppIcon(AppIcons.uiExternalLink, size: 13)
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        }
                    }
                }
                .accessibilityIdentifier(rowIdentifier)
            }
        }
    }
}

/// One member of a linear stack.
public struct StackRailMember: Identifiable {
    public let id: String
    public let identifier: String
    public let title: String
    /// The member the page is showing: the filled node + the active wash.
    public let current: Bool

    public init(id: String, identifier: String, title: String, current: Bool = false) {
        self.id = id
        self.identifier = identifier
        self.title = title
        self.current = current
    }
}

/// A STACK, top-first, on one rail down to its base-branch row. Phones reach
/// "Merge through here" from a member's long-press menu (`onMergeThrough`);
/// there is no hover.
public struct StackRail: View {
    private let members: [StackRailMember]
    private let baseBranch: String
    private let word: String?
    private let onOpen: ((StackRailMember) -> Void)?
    private let onMergeThrough: ((StackRailMember) -> Void)?
    private let mergeThroughLabel: String
    private let rowIdentifier: String

    /// - Parameters:
    ///   - members: top member first.
    ///   - word: the quiet word on the top row (`stack` in Reviews; none on
    ///     the Guide).
    public init(
        members: [StackRailMember],
        baseBranch: String,
        word: String? = nil,
        onOpen: ((StackRailMember) -> Void)? = nil,
        onMergeThrough: ((StackRailMember) -> Void)? = nil,
        mergeThroughLabel: String = DomainContract.diffUiMergeThrough,
        rowIdentifier: String = "pr-row"
    ) {
        self.members = members
        self.baseBranch = baseBranch
        self.word = word
        self.onOpen = onOpen
        self.onMergeThrough = onMergeThrough
        self.mergeThroughLabel = mergeThroughLabel
        self.rowIdentifier = rowIdentifier
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(members.enumerated()), id: \.element.id) { index, member in
                PrTapTarget(onOpen: onOpen.map { open in { open(member) } }) {
                    PrRow(
                        node: member.current ? .current : .open,
                        identifier: member.identifier,
                        title: member.title,
                        word: index == 0 ? word : nil,
                        rail: PrRailSegments(above: index > 0, below: true),
                        active: member.current
                    )
                }
                .modifier(MergeThroughMenu(label: mergeThroughLabel, action: onMergeThrough.map { merge in { merge(member) } }))
                .accessibilityIdentifier(rowIdentifier)
            }
            PrRow(node: .base, title: baseBranch, rail: PrRailSegments(above: !members.isEmpty))
                .accessibilityIdentifier("pr-row-base")
        }
    }
}

/// The long-press "Merge through here" on a stack member (phones; desktop and
/// web show it as a hover ghost).
private struct MergeThroughMenu: ViewModifier {
    let label: String
    let action: (() -> Void)?

    func body(content: Content) -> some View {
        if let action {
            content.contextMenu {
                Button(action: action) {
                    Label(label, appIcon: AppIcons.prMerged)
                }
            }
        } else {
            content
        }
    }
}

/// A row's tap: a plain Button when it opens something, the bare row else.
private struct PrTapTarget<Content: View>: View {
    let onOpen: (() -> Void)?
    @ViewBuilder let content: () -> Content

    var body: some View {
        if let onOpen {
            Button(action: onOpen) { content() }
                .buttonStyle(.plain)
        } else {
            content()
        }
    }
}
