import SwiftUI

/// A scroll container (contract §2 `overflowX/Y: scroll | auto`, a windowed
/// `List`): the children at their UNSCROLLED frames inside a SwiftUI
/// `ScrollView` on the scrolling axes (momentum, indicators, wheel and
/// trackpad on macOS for free), the content as large as the core says.
/// The offset goes back to the core (`scrollTo`: anchored layers and
/// `scrollIntoView` read it; a windowed list re-windows in constant work);
/// an offset the core moved itself scrolls the view there. Nothing here
/// guesses row heights.
struct ScrollBox: View {
    let index: Int
    let model: SurfaceModel
    let size: CGSize
    let kids: [Int]
    let origin: CGPoint
    let scroll: PaintScroll
    /// The last offset reported to the core (a reference: scrolling never
    /// re-renders the view).
    @State private var tracker = ScrollTracker()

    private var space: String { "xui-scroll-\(index)" }
    private var anchorId: String { "xui-scroll-anchor-\(index)" }

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView(scroll.axes, showsIndicators: true) {
                ZStack(alignment: .topLeading) {
                    ChildrenLayout(size: scroll.contentSize, kids: kids, origin: origin, model: model)
                    if let o = scroll.offset {
                        // A 1-pt anchor at the core's offset: `scrollTo`
                        // works on layout positions, so it is PADDED there.
                        Color.clear.frame(width: 1, height: 1)
                            .padding(.leading, o.x)
                            .padding(.top, o.y)
                            .id(anchorId)
                            .accessibilityHidden(true)
                    }
                }
                .frame(width: scroll.contentSize.width, height: scroll.contentSize.height, alignment: .topLeading)
                .background {
                    GeometryReader { proxy in
                        let f = proxy.frame(in: .named(space))
                        Color.clear.preference(key: ScrollBoxOffsetKey.self, value: CGPoint(x: -f.minX, y: -f.minY))
                    }
                }
            }
            .coordinateSpace(name: space)
            .onPreferenceChange(ScrollBoxOffsetKey.self) { offset in
                let maxX = max(0, scroll.contentSize.width - size.width)
                let maxY = max(0, scroll.contentSize.height - size.height)
                // Rubber-banding past an edge reports the clamped offset.
                let clamped = CGPoint(x: min(maxX, max(0, offset.x)), y: min(maxY, max(0, offset.y)))
                if clamped == tracker.reported { return }
                tracker.reported = clamped
                model.paintScrolled(index, to: clamped)
            }
            .onChange(of: scroll.offset) { _, o in
                // Only an offset the CORE moved (not the echo of ours).
                if let o, abs(o.x - tracker.reported.x) > 1 || abs(o.y - tracker.reported.y) > 1 {
                    proxy.scrollTo(anchorId, anchor: .topLeading)
                }
            }
        }
        .frame(width: size.width, height: size.height)
    }
}

private final class ScrollTracker {
    var reported: CGPoint = .zero
}

private struct ScrollBoxOffsetKey: PreferenceKey {
    static let defaultValue: CGPoint = .zero
    static func reduce(value: inout CGPoint, nextValue: () -> CGPoint) { value = nextValue() }
}
