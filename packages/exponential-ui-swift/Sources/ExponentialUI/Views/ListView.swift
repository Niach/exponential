import SwiftUI

/// A scrolling container (`overflow: scroll` on either axis, or a `List` /
/// `Table` the core windowed): the children at their content offsets inside
/// a NATIVE scroll view (momentum, indicators) whose content is as large as
/// the core says. The frames are unscrolled: the scroll view moves them and
/// reports its offset back (`scrollReported`), so the core's offsets stay
/// true (scrollIntoView, keyboard scrolling, windowing — a windowed list
/// moves its window in constant work). When the CORE moves an offset (a
/// command, the keyboard, a clamp) the view follows through
/// `model.scrollJumps`. Nothing here guesses row heights.
struct ScrollContainer: View {
    let index: Int
    let node: NodeInfo
    let model: SurfaceModel
    let size: CGSize
    let kids: [Int]
    let origin: CGPoint

    var body: some View {
        let info = model.scroll(index)
        let horizontal = info?.scrollsX ?? false
        let vertical = info?.scrollsY ?? true
        let axes: Axis.Set = horizontal && vertical ? [.horizontal, .vertical] : (horizontal ? .horizontal : .vertical)
        let content = CGSize(
            width: horizontal ? max(model.contentWidth(index), size.width) : size.width,
            height: vertical ? max(model.contentHeight(index), size.height) : size.height
        )
        let space = "xui-scroll-\(node.id)"
        let anchor = "xui-scroll-anchor-\(node.id)"
        let jump = model.scrollJumps[index]
        ScrollViewReader { proxy in
            ScrollView(axes, showsIndicators: true) {
                ZStack(alignment: .topLeading) {
                    ChildrenLayout(size: content, kids: kids, origin: origin, model: model)
                    // The target of a programmatic jump: a point at the
                    // core's offset (iOS 17 scrolls to views, not offsets).
                    Color.clear
                        .frame(width: 1, height: 1)
                        .id(anchor)
                        .padding(.leading, min(jump?.offset.x ?? 0, max(0, content.width - 1)))
                        .padding(.top, min(jump?.offset.y ?? 0, max(0, content.height - 1)))
                        .accessibilityHidden(true)
                        .allowsHitTesting(false)
                }
                .frame(width: content.width, height: content.height, alignment: .topLeading)
                .background {
                    GeometryReader { proxy in
                        let f = proxy.frame(in: .named(space))
                        Color.clear.preference(key: ScrollOffsetKey.self, value: CGPoint(x: max(0, -f.minX), y: max(0, -f.minY)))
                    }
                }
            }
            .coordinateSpace(name: space)
            .onPreferenceChange(ScrollOffsetKey.self) { offset in
                model.scrollReported(index, offset: offset)
            }
            .onChange(of: jump) { _, j in
                guard j != nil else { return }
                proxy.scrollTo(anchor, anchor: .topLeading)
            }
        }
        .frame(width: size.width, height: size.height)
    }
}

private struct ScrollOffsetKey: PreferenceKey {
    static let defaultValue: CGPoint = .zero
    static func reduce(value: inout CGPoint, nextValue: () -> CGPoint) { value = nextValue() }
}
