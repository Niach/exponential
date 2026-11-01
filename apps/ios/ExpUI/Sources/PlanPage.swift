import ExpCore
import SwiftUI

// #EXP-1493 THE plan page's parts ×4 (fixtures/plan-page.json + stories.json
// `badge`; web packages/ui plan-page.tsx, desktop plan_page.rs, Android
// ui/plan/PlanPage.kt): the kind badge, the phone picture group (THE eUI
// Carousel: one slide per picture, "i / n" top-left, dots under it), THE
// outline (eUI `Outline`: the caption, one row per section, the questions
// row) and the thumbnail strip the story viewer's plan cover and gallery
// filmstrip share. The page itself (markdown, Dock, lightbox) lives in the
// app (`PlanPageScreen`): those renderers are the app's.

extension Stories.Kind {
    /// The ask-dock tints: plan primary, question yellow, a result the
    /// closed-check green.
    public var tint: Color {
        switch self {
        case .plan: DesignTokens.Palette.primary
        case .question: DesignTokens.Semantic.yellow
        case .result: DesignTokens.Semantic.green
        }
    }
}

/// THE kind badge (stories.json `badge` = THE Pill, sm, readonly): the
/// concept icon + the kind word in the tint, the tint at `fill` behind and
/// at `stroke` as its border. `waiting` = the plan page's waiting dot after
/// it (plan-page.json: a 6 pt needs-you yellow dot, no word; #1455's token).
public struct StoryKindBadge: View {
    let kind: Stories.Kind
    var waiting = false

    public static let waitingDot: CGFloat = 6

    public init(kind: Stories.Kind, waiting: Bool = false) {
        self.kind = kind
        self.waiting = waiting
    }

    public var body: some View {
        HStack(spacing: 8) {
            pill
            if waiting {
                Circle()
                    .fill(DesignTokens.Semantic.yellow)
                    .frame(width: Self.waitingDot, height: Self.waitingDot)
                    .accessibilityHidden(true)
            }
        }
        .fixedSize()
    }

    private var pill: some View {
        let tint = kind.tint
        return HStack(spacing: 4) {
            AppIcon(AppIcons.conceptOrName(Stories.Badge.icon(kind)) ?? AppIcons.codingPlan, size: 11, weight: .semibold)
            Text(Stories.Copy.kind(kind))
                .font(.caption2.weight(.medium))
                .lineLimit(1)
        }
        .foregroundStyle(tint)
        .padding(.horizontal, 8)
        .padding(.vertical, 3)
        .background(Capsule().fill(tint.opacity(Stories.Badge.fill)))
        .overlay(Capsule().stroke(tint.opacity(Stories.Badge.stroke), lineWidth: 1))
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("story-badge")
    }
}

/// THE phone picture group (eUI Carousel): one slide per picture at the
/// group's aspect (the first picture's), the "i / n" counter top-left, the
/// dots under it, swipe. A tap on a slide = `onOpen(index)` (the lightbox).
public struct PlanPictureCarousel<Slide: View>: View {
    let count: Int
    let aspect: CGFloat
    let onOpen: (Int) -> Void
    let slide: (Int) -> Slide

    @State private var page: Int? = 0

    public init(count: Int, aspect: CGFloat, onOpen: @escaping (Int) -> Void, @ViewBuilder slide: @escaping (Int) -> Slide) {
        self.count = count
        self.aspect = aspect
        self.onOpen = onOpen
        self.slide = slide
    }

    public var body: some View {
        let radius = CGFloat(PlanPage.Mosaic.radius)
        VStack(spacing: 8) {
            // The group's box = the first picture's aspect at the column's
            // width (a slide still loading keeps its height).
            Color.clear
                .aspectRatio(aspect, contentMode: .fit)
                .frame(maxWidth: .infinity)
                .overlay { slides(radius) }
            dots
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(PlanPage.TestIds.carousel)
    }

    private func slides(_ radius: CGFloat) -> some View {
            ScrollView(.horizontal) {
                LazyHStack(spacing: 8) {
                    ForEach(0..<count, id: \.self) { index in
                        Button { onOpen(index) } label: {
                            slide(index)
                                .frame(maxWidth: .infinity, maxHeight: .infinity)
                                .background(Color.black)
                                .clipShape(RoundedRectangle(cornerRadius: radius))
                                .overlay(
                                    RoundedRectangle(cornerRadius: radius)
                                        .stroke(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline)
                                )
                                .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .containerRelativeFrame([.horizontal, .vertical])
                        .id(index)
                    }
                }
                .scrollTargetLayout()
            }
            .scrollTargetBehavior(.viewAligned)
            .scrollPosition(id: $page)
            .scrollIndicators(.hidden)
            .overlay(alignment: .topLeading) {
                Text("\((page ?? 0) + 1) / \(count)")
                    .font(.caption2.weight(.semibold).monospacedDigit())
                    .foregroundStyle(.white)
                    .padding(.horizontal, 7)
                    .padding(.vertical, 2)
                    .background(Capsule().fill(Color.black.opacity(0.55)))
                    .padding(8)
                    .allowsHitTesting(false)
                    .accessibilityIdentifier("plan-carousel-counter")
            }
    }

    private var dots: some View {
            HStack(spacing: 5) {
                ForEach(0..<count, id: \.self) { index in
                    Circle()
                        .fill(index == (page ?? 0) ? Color.white : Color.white.opacity(0.3))
                        .frame(width: 5, height: 5)
                }
            }
            .accessibilityHidden(true)
    }
}

/// THE outline (eUI `Outline`): the caption, one row per section (the
/// title one line, a muted `· n` with pictures; the active row = the accent
/// background + the leading bar), then a separator and the questions row.
public struct PlanOutlineList: View {
    let sections: [PlanPage.Section]
    let active: String?
    /// The plan's question count; 0 = no questions row.
    let questions: Int
    let onSelect: (String) -> Void
    let onQuestions: () -> Void

    public init(
        sections: [PlanPage.Section], active: String?, questions: Int,
        onSelect: @escaping (String) -> Void, onQuestions: @escaping () -> Void
    ) {
        self.sections = sections
        self.active = active
        self.questions = questions
        self.onSelect = onSelect
        self.onQuestions = onQuestions
    }

    public var body: some View {
        let rowHeight = CGFloat(PlanPage.Layout.outlineRowHeight)
        VStack(alignment: .leading, spacing: 2) {
            Text(PlanPage.Copy.outline.uppercased())
                .font(.caption2.weight(.medium))
                .tracking(0.6)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .padding(.horizontal, 10)
                .padding(.bottom, 6)
            ForEach(sections) { section in
                let on = section.id == active
                Button { onSelect(section.id) } label: {
                    HStack(spacing: 0) {
                        Text(section.title)
                            .lineLimit(1)
                            .truncationMode(.tail)
                            .foregroundStyle(.white.opacity(on ? 1 : TextOpacity.secondary))
                        if section.pictures > 0 {
                            Text(" · \(section.pictures)")
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                .fixedSize()
                        }
                        Spacer(minLength: 0)
                    }
                    .font(.subheadline)
                    .padding(.horizontal, 10)
                    .frame(height: rowHeight)
                    .background(on ? GlassTokens.fillActive : Color.clear, in: RoundedRectangle(cornerRadius: GlassTokens.rowRadius))
                    .overlay(alignment: .leading) {
                        if on {
                            Rectangle()
                                .fill(Color.white)
                                .frame(width: CGFloat(PlanPage.Layout.outlineActiveBar))
                                .padding(.vertical, 6)
                        }
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier(PlanPage.TestIds.outlineRow)
            }
            if questions > 0 {
                Rectangle()
                    .fill(GlassTokens.strokeCard)
                    .frame(height: GlassTokens.hairline)
                    .padding(.vertical, 6)
                Button(action: onQuestions) {
                    HStack(spacing: 0) {
                        Text(PlanPage.Copy.questions)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        Text(" · \(questions)")
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        Spacer(minLength: 0)
                    }
                    .font(.subheadline)
                    .padding(.horizontal, 10)
                    .frame(height: rowHeight)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(PlanPage.TestIds.outline)
    }
}

/// The story viewer's thumbnail row: the plan cover's (40 pt tall, up to 4
/// + `+N` muted) and the gallery filmstrip (52 pt squares); the current =
/// a 2 pt foreground ring. A tap = `onPick(index)`.
public struct StoryThumbnailStrip<Thumb: View>: View {
    let count: Int
    let current: Int?
    let height: CGFloat
    /// nil = square tiles.
    let width: CGFloat?
    let limit: Int?
    let onPick: (Int) -> Void
    let thumb: (Int) -> Thumb

    public init(
        count: Int, current: Int?, height: CGFloat, width: CGFloat? = nil, limit: Int? = nil,
        onPick: @escaping (Int) -> Void, @ViewBuilder thumb: @escaping (Int) -> Thumb
    ) {
        self.count = count
        self.current = current
        self.height = height
        self.width = width
        self.limit = limit
        self.onPick = onPick
        self.thumb = thumb
    }

    public var body: some View {
        let shown = min(count, limit ?? count)
        let radius = CGFloat(Stories.Tab.Tile.radius) * 0.8
        HStack(spacing: 8) {
            ForEach(0..<shown, id: \.self) { index in
                Button { onPick(index) } label: {
                    thumb(index)
                        .frame(width: width ?? height, height: height)
                        .clipShape(RoundedRectangle(cornerRadius: radius))
                        .overlay(
                            RoundedRectangle(cornerRadius: radius)
                                .stroke(
                                    index == current ? Color.white : GlassTokens.strokeCard,
                                    lineWidth: index == current ? 2 : GlassTokens.hairline
                                )
                        )
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
            if shown < count {
                Text("+\(count - shown)")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
        }
    }
}
