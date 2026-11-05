import ExpCore
import SwiftUI

// #EXP-1493 → #EXP-1662 THE plan tab's parts ×4 (fixtures/plan-page.json; web
// packages/ui plan-page.tsx, desktop plan_page.rs, Android ui/plan/PlanPage.kt):
// the phone picture group (THE eUI Carousel: one slide per picture, "i / n"
// top-left, dots under it), THE outline (eUI `Outline`: the caption, one row per
// section, the questions row) and the floating panel's surface. The tab's body (markdown,
// the questions, lightbox) lives in the app (`PlanPageBody`): those renderers
// are the app's.

/// #EXP-1662 THE floating panel's surface (plan-page.json `panel`): centred,
/// `maxWidth` wide, the menu surface fill + hairline + `radius`, `padding`; it
/// hugs its content up to `maxHeight` and scrolls inside past that, the header
/// and the footer pinned.
public struct PlanFloatingPanel<Header: View, Content: View, Footer: View>: View {
    let maxHeight: CGFloat
    let header: Header
    let content: Content
    let footer: Footer

    @State private var contentHeight: CGFloat = 0

    public init(
        maxHeight: CGFloat,
        @ViewBuilder header: () -> Header,
        @ViewBuilder content: () -> Content,
        @ViewBuilder footer: () -> Footer
    ) {
        self.maxHeight = maxHeight
        self.header = header()
        self.content = content()
        self.footer = footer()
    }

    public var body: some View {
        let shape = RoundedRectangle(cornerRadius: CGFloat(PlanPage.Panel.radius), style: .continuous)
        CappedHeight(maxHeight: maxHeight) {
            VStack(spacing: 0) {
                header
                ScrollView {
                    content
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .onGeometryChange(for: CGFloat.self, of: { $0.size.height }) { contentHeight = $0 }
                }
                .scrollBounceBehavior(.basedOnSize)
                .scrollIndicators(.hidden)
                .frame(maxHeight: contentHeight)
                footer
                    .frame(maxWidth: .infinity, alignment: .trailing)
                    .padding(.top, contentHeight > 0 ? 12 : 0)
            }
            .padding(CGFloat(PlanPage.Panel.padding))
        }
        .frame(maxWidth: CGFloat(PlanPage.Panel.maxWidth))
        .background(GlassTokens.opaqueCardFill, in: shape)
        .overlay(shape.strokeBorder(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline))
        .shadow(color: .black.opacity(0.3), radius: 12, y: 6)
    }
}

/// Proposes at most `maxHeight` to its one child and reports the child's own
/// size: a `.frame(maxHeight:)` would grow to it instead of hugging.
private struct CappedHeight: Layout {
    let maxHeight: CGFloat

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        guard let child = subviews.first else { return .zero }
        return child.sizeThatFits(ProposedViewSize(width: proposal.width, height: min(proposal.height ?? maxHeight, maxHeight)))
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        subviews.first?.place(at: bounds.origin, proposal: ProposedViewSize(bounds.size))
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

    public init(
        sections: [PlanPage.Section], active: String?, questions: Int,
        onSelect: @escaping (String) -> Void
    ) {
        self.sections = sections
        self.active = active
        self.questions = questions
        self.onSelect = onSelect
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
                // Scrolls nothing: the panel already shows them.
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
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(PlanPage.TestIds.outline)
    }
}
