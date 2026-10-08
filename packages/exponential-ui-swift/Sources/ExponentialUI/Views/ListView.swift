import SwiftUI

/// A scrolling container (`overflow: scroll`, or a `List` the core
/// windowed): the children at their content offsets inside a scroll view
/// whose content is as tall as the core says. A windowed list reports its
/// visible offset back (`scroll(list, offset)`), which moves the window in
/// constant work; nothing here guesses row heights.
struct ScrollContainer: View {
    let index: Int
    let node: NodeInfo
    let model: SurfaceModel
    let size: CGSize
    let kids: [Int]
    let origin: CGPoint

    private var contentHeight: CGFloat { max(model.contentHeight(index), size.height) }

    var body: some View {
        let listId = node.id
        let windowed = model.lists[listId]?.windowed ?? false
        ScrollView(.vertical, showsIndicators: true) {
            ChildrenLayout(size: CGSize(width: size.width, height: contentHeight), kids: kids, origin: origin, model: model)
                .background {
                    if windowed {
                        GeometryReader { proxy in
                            Color.clear.preference(key: ScrollOffsetKey.self, value: -proxy.frame(in: .named("xui-scroll-\(listId)")).minY)
                        }
                    }
                }
        }
        .coordinateSpace(name: "xui-scroll-\(listId)")
        .onPreferenceChange(ScrollOffsetKey.self) { offset in
            if windowed { model.scroll(list: listId, offset: max(0, offset)) }
        }
        .frame(width: size.width, height: size.height)
    }
}

private struct ScrollOffsetKey: PreferenceKey {
    static let defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) { value = nextValue() }
}
