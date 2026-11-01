import SwiftUI

/// A scroll container (contract §2 `overflowX/Y: scroll | auto`, the core's
/// `FfiScroll`, or a `List` / `Table` the core windowed): the children at
/// their UNSCROLLED frames inside a native `ScrollView` on the scrolling
/// axes (momentum, indicators, wheel and trackpad on macOS), the content as
/// large as the core says. The view reports its offset back
/// (`scrollReported`): the core owns every offset (anchored layers,
/// `scrollIntoView`; a windowed list re-windows in constant work). When the
/// CORE moves an offset (a command, the keyboard, a clamp) the view follows
/// `model.scrollJumps`. Nothing here guesses row heights.
struct ScrollContainer: View {
    let index: Int
    let node: NodeInfo
    let model: SurfaceModel
    let size: CGSize
    let kids: [Int]
    let origin: CGPoint
    /// The last offset reported (a reference: scrolling never re-renders).
    @State private var tracker = ScrollTracker()

    var body: some View {
        let info = model.scroll(index)
        // A windowed List scrolls along its own axis (round 2: either one).
        let listX = model.lists[node.id].map { $0.windowed && $0.horizontal } ?? false
        let horizontal = info?.scrollsX ?? listX
        let vertical = info?.scrollsY ?? !listX
        let axes: Axis.Set = horizontal && vertical ? [.horizontal, .vertical] : (horizontal ? .horizontal : .vertical)
        let content = CGSize(
            width: horizontal ? max(model.contentWidth(index), size.width) : size.width,
            height: vertical ? max(model.contentHeight(index), size.height) : size.height
        )
        let space = "xui-scroll-\(node.id)"
        let anchor = "xui-scroll-anchor-\(node.id)"
        let jump = model.scrollJumps[index]
        let follow = ThumbnailFollow.target(node: node, index: index, model: model)
        let followAnchor = "xui-scroll-follow-\(node.id)"
        ScrollViewReader { proxy in
            ScrollView(axes, showsIndicators: true) {
                ZStack(alignment: .topLeading) {
                    ChildrenLayout(size: content, kids: kids, origin: origin, model: model)
                    // The target of a programmatic jump: a point PADDED to
                    // the core's offset (iOS 17 scrolls to views, not offsets).
                    Color.clear
                        .frame(width: 1, height: 1)
                        .padding(.leading, min(jump?.offset.x ?? 0, max(0, content.width - 1)))
                        .padding(.top, min(jump?.offset.y ?? 0, max(0, content.height - 1)))
                        .id(anchor)
                        .accessibilityHidden(true)
                        .allowsHitTesting(false)
                    if let follow {
                        Color.clear
                            .frame(width: follow.width, height: 1)
                            .padding(.leading, follow.minX)
                            .id(followAnchor)
                            .accessibilityHidden(true)
                            .allowsHitTesting(false)
                    }
                }
                .frame(width: content.width, height: content.height, alignment: .topLeading)
                .background {
                    GeometryReader { proxy in
                        let f = proxy.frame(in: .named(space))
                        Color.clear.preference(key: ScrollOffsetKey.self, value: CGPoint(x: -f.minX, y: -f.minY))
                    }
                }
            }
            .coordinateSpace(name: space)
            .onPreferenceChange(ScrollOffsetKey.self) { offset in
                // Rubber-banding past an edge reports the clamped offset.
                let clamped = CGPoint(
                    x: min(max(0, content.width - size.width), max(0, offset.x)),
                    y: min(max(0, content.height - size.height), max(0, offset.y))
                )
                if clamped == tracker.reported { return }
                tracker.reported = clamped
                model.scrollReported(index, offset: clamped)
            }
            .onChange(of: jump) { _, j in
                guard j != nil else { return }
                proxy.scrollTo(anchor, anchor: .topLeading)
            }
            // EXP-1495: a Carousel's thumbnail strip keeps the current
            // page's thumbnail in view (`nearest`: no anchor = the least
            // scroll that shows it); the new offset reaches the core through
            // the usual report.
            .onAppear { if follow != nil { proxy.scrollTo(followAnchor) } }
            .onChange(of: follow) { _, f in
                guard f != nil else { return }
                if model.reducedMotion {
                    proxy.scrollTo(followAnchor)
                } else {
                    withAnimation(.easeOut(duration: 0.2)) { proxy.scrollTo(followAnchor) }
                }
            }
        }
        .frame(width: size.width, height: size.height)
    }
}

/// Round-6 §3: where the Carousel `thumbnails` strip's current thumbnail
/// sits inside the strip's content (nil for any other scroller).
@MainActor
enum ThumbnailFollow {
    static func target(node: NodeInfo, index: Int, model: SurfaceModel) -> CGRect? {
        guard node.ownerComponent == "Carousel", node.part == "thumbnails", let owner = node.owner,
              let page = node.props.num("page"), let i = model.index(of: "\(owner).thumbnail.\(Int(page))") else { return nil }
        let f = model.frame(i)
        return CGRect(x: f.minX - model.frame(index).minX, y: 0, width: f.width, height: f.height)
    }
}

private final class ScrollTracker {
    var reported: CGPoint = .zero
}

private struct ScrollOffsetKey: PreferenceKey {
    static let defaultValue: CGPoint = .zero
    static func reduce(value: inout CGPoint, nextValue: () -> CGPoint) { value = nextValue() }
}
