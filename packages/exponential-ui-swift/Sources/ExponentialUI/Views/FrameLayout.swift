import SwiftUI

/// Tags every subview with its node index.
struct NodeIndexKey: LayoutValueKey {
    static let defaultValue: Int = -1
}

/// The ONE layout: every child is placed at the frame the core computed
/// (relative to this container), proposing exactly that size. SwiftUI only
/// places; it never measures through here (the measurer answered the core
/// in batches before the pass). Coordinates are PHYSICAL: the surface root
/// pins `layoutDirection` to left-to-right so an RTL surface, already
/// resolved by the core, is not mirrored a second time.
struct FrameLayout: Layout {
    let size: CGSize
    let frames: [Int: CGRect]

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        size
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        for subview in subviews {
            let index = subview[NodeIndexKey.self]
            guard let f = frames[index] else {
                subview.place(at: bounds.origin, anchor: .topLeading, proposal: ProposedViewSize(width: 0, height: 0))
                continue
            }
            subview.place(at: CGPoint(x: bounds.minX + f.minX, y: bounds.minY + f.minY), anchor: .topLeading, proposal: ProposedViewSize(width: f.width, height: f.height))
        }
    }
}
