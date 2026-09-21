import ExpCore
import SwiftUI

// EXP-920 — the entity preview CARD, presentational half. A tap on an entity
// chip (`EntityChip`) opens one of these as a fitted glass sheet: the kind's
// glyph, an eyebrow (the kind noun or the identifier), the title, an optional
// subtitle, an optional expandable excerpt, a row of fact pills, a short list
// of related rows (a board's issues, a list chip's members) with a `+N more`
// tail, and ONE primary "Open" that leaves for the entity's own screen. The
// app side (`EntityRefPreviewSheet`) resolves every field off its synced rows
// and decides where Open goes; this view only paints.

/// One related row in the card: a glyph, a primary text, an optional
/// secondary text, and a tap when the row is something the reader can open.
public struct EntityPreviewRow: Identifiable {
    public let id: String
    public let icon: AnyView?
    public let primary: String
    public let secondary: String?
    public let onTap: (() -> Void)?

    public init(
        id: String,
        icon: AnyView? = nil,
        primary: String,
        secondary: String? = nil,
        onTap: (() -> Void)? = nil
    ) {
        self.id = id
        self.icon = icon
        self.primary = primary
        self.secondary = secondary
        self.onTap = onTap
    }
}

/// The card's own numbers.
public enum EntityPreviewCardTokens {
    /// Excerpt lines shown before "Show more".
    public static let collapsedExcerptLines = 4
    /// An excerpt at or under this many characters never needs the toggle.
    public static let excerptToggleThreshold = 160
}

public struct EntityPreviewCard<Icon: View, Facts: View>: View {
    let icon: Icon
    let eyebrow: String?
    let title: String
    let subtitle: String?
    let excerpt: String?
    let facts: Facts
    let rows: [EntityPreviewRow]
    let more: String?
    let onOpen: (() -> Void)?

    @State private var expanded = false

    /// - Parameters:
    ///   - eyebrow: the small line above the title (the kind noun, an
    ///     identifier).
    ///   - excerpt: a longer body (a comment, a description) — four lines,
    ///     then a "Show more" toggle.
    ///   - facts: pills (`GlassPill`) laid out by the card in one row.
    ///   - rows: related entities, each its own glass row.
    ///   - more: the tail under the rows (`+3 more`).
    ///   - onOpen: the primary action; nil = nothing to open (a list, an
    ///     entity this phone does not sync).
    public init(
        eyebrow: String? = nil,
        title: String,
        subtitle: String? = nil,
        excerpt: String? = nil,
        rows: [EntityPreviewRow] = [],
        more: String? = nil,
        onOpen: (() -> Void)? = nil,
        @ViewBuilder icon: () -> Icon,
        @ViewBuilder facts: () -> Facts
    ) {
        self.icon = icon()
        self.eyebrow = eyebrow
        self.title = title
        self.subtitle = subtitle
        self.excerpt = excerpt
        self.facts = facts()
        self.rows = rows
        self.more = more
        self.onOpen = onOpen
    }

    public var body: some View {
        if let onOpen {
            GlassSheetChrome(height: .fitted) {
                content
            } primaryAction: {
                GlassSubmitButton("Open", action: onOpen)
            }
            .accessibilityIdentifier("entity-preview-sheet")
        } else {
            GlassSheetChrome(height: .fitted) {
                content
            }
            .accessibilityIdentifier("entity-preview-sheet")
        }
    }

    private var content: some View {
        VStack(alignment: .leading, spacing: 12) {
            header
            if let excerpt, !excerpt.isEmpty {
                excerptBlock(excerpt)
            }
            if Facts.self != EmptyView.self {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 6) { facts }
                }
            }
            if !rows.isEmpty {
                VStack(spacing: 2) {
                    ForEach(rows) { row in
                        rowView(row)
                    }
                }
            }
            if let more, !more.isEmpty {
                Text(more)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .padding(.horizontal, 12)
            }
        }
        .padding(.horizontal, 16)
        .padding(.top, GlassSheetTokens.headerTopPadding)
        .padding(.bottom, 16)
    }

    private var header: some View {
        HStack(alignment: .top, spacing: 12) {
            icon
                .frame(width: 32, height: 32)
                .background(GlassTokens.fillSection)
                .clipShape(RoundedRectangle(cornerRadius: GlassTokens.rowRadius, style: .continuous))
            VStack(alignment: .leading, spacing: 2) {
                if let eyebrow, !eyebrow.isEmpty {
                    Text(eyebrow)
                        .font(.caption2.monospaced())
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .lineLimit(1)
                }
                Text(title)
                    .font(.headline)
                    .foregroundStyle(.white)
                    .lineLimit(3)
                    .fixedSize(horizontal: false, vertical: true)
                if let subtitle, !subtitle.isEmpty {
                    Text(subtitle)
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .lineLimit(2)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            Spacer(minLength: 0)
        }
        .accessibilityElement(children: .combine)
    }

    /// Four lines of the excerpt, then the whole thing on a tap — an agent's
    /// comment can run long, and the card should still fit the reader's
    /// thumb.
    private var collapsedLines: Int { EntityPreviewCardTokens.collapsedExcerptLines }

    @ViewBuilder
    private func excerptBlock(_ text: String) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(text)
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .lineLimit(expanded ? nil : collapsedLines)
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, alignment: .leading)
            if !expanded, text.count > EntityPreviewCardTokens.excerptToggleThreshold || text.filter({ $0.isNewline }).count >= collapsedLines {
                Button {
                    expanded = true
                } label: {
                    Text("Show more")
                        .font(.caption.weight(.medium))
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                }
                .buttonStyle(.plain)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .glassRow()
    }

    @ViewBuilder
    private func rowView(_ row: EntityPreviewRow) -> some View {
        if let onTap = row.onTap {
            Button(action: onTap) {
                rowBody(row).contentShape(Rectangle())
            }
            .buttonStyle(.plain)
        } else {
            rowBody(row)
        }
    }

    private func rowBody(_ row: EntityPreviewRow) -> some View {
        HStack(spacing: 10) {
            if let icon = row.icon {
                icon.frame(width: 20)
            }
            VStack(alignment: .leading, spacing: 1) {
                Text(row.primary)
                    .font(.subheadline)
                    .foregroundStyle(.white)
                    .lineLimit(1)
                if let secondary = row.secondary, !secondary.isEmpty {
                    Text(secondary)
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .lineLimit(1)
                }
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .frame(minHeight: 40)
        .glassRow()
    }
}

extension EntityPreviewCard where Facts == EmptyView {
    public init(
        eyebrow: String? = nil,
        title: String,
        subtitle: String? = nil,
        excerpt: String? = nil,
        rows: [EntityPreviewRow] = [],
        more: String? = nil,
        onOpen: (() -> Void)? = nil,
        @ViewBuilder icon: () -> Icon
    ) {
        self.init(
            eyebrow: eyebrow,
            title: title,
            subtitle: subtitle,
            excerpt: excerpt,
            rows: rows,
            more: more,
            onOpen: onOpen,
            icon: icon,
            facts: { EmptyView() }
        )
    }
}
