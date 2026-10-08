import SwiftUI
import ExponentialUICore

/// What the painter reads off the model beyond frames and visuals: the
/// surface direction, reduced motion, per-node scroll containers and the
/// layer flags. Each accessor prefers the model's stored round-1 data and
/// otherwise derives the same answer from the core (the facade's settings,
/// the resolved visuals, the layer kinds), so the painter never guesses
/// what the core decided.
///
/// VAPP-100 L2: the stored data (`FfiLayout.direction`, `.scrolls`,
/// `FfiLayer.class/modal/dismissible`) lands on the model in lane L1; once
/// it does, these bodies read it (see the lane report).
@MainActor
extension SurfaceModel {
    /// The surface direction (contract §4): `rtl` when the core laid the
    /// tree out right-to-left. Frames already come mirrored; the painter
    /// only flips text direction and the `rtlMirroredIcons` glyphs.
    var paintsRTL: Bool {
        PaintCache.shared.entry(self).rtl
    }

    /// Reduced motion as the surface knows it (the host's setting through
    /// `setPointer` / `setSettings`); the views also honour the platform's
    /// `accessibilityReduceMotion`.
    var paintReducedMotion: Bool {
        PaintCache.shared.entry(self).reducedMotion
    }

    /// A scroll container: the axes that scroll (the core reports
    /// `overflow: scroll | auto` per axis), the content extent and the
    /// core's clamped offset (frames are UNSCROLLED).
    func paintScroll(_ index: Int) -> PaintScroll? {
        guard let n = node(index), !n.isLeaf else { return nil }
        let s = style(index)
        let windowed = lists[n.id]?.windowed ?? false
        guard s.scrolls || windowed else { return nil }
        let f = frame(index)
        var w: CGFloat = f.width, h: CGFloat = f.height
        for c in children[safe: index] ?? [] where !(node(c)?.hidden ?? true) {
            let cf = frame(c)
            w = max(w, cf.maxX - f.minX)
            h = max(h, cf.maxY - f.minY)
        }
        if windowed, let l = lists[n.id] { h = max(l.contentHeight, f.height) }
        let x = s.scrollX, y = s.scrollY || windowed || (!s.scrollX && s.scrolls)
        return PaintScroll(index: index, scrollX: x, scrollY: y, contentSize: CGSize(width: x ? w : f.width, height: y ? h : f.height), offset: nil, windowed: windowed)
    }

    /// Report a container's scroll offset to the core (`scrollTo`): the
    /// core owns offsets (anchored layers, `scrollIntoView`); a windowed
    /// list re-windows.
    func paintScrolled(_ index: Int, to offset: CGPoint) {
        guard let n = node(index) else { return }
        if lists[n.id]?.windowed ?? false {
            scroll(list: n.id, offset: offset.y)
            return
        }
        if surface.scrollTo(id: n.id, x: Float(offset.x), y: Float(offset.y)), !layers.isEmpty {
            // An anchored layer inside the container follows it.
            layoutNeeded = true
            pass()
        }
    }
}

/// One scroll container (the facade's `FfiScroll`, painter-side).
struct PaintScroll: Equatable {
    var index: Int
    var scrollX: Bool
    var scrollY: Bool
    var contentSize: CGSize
    /// The core's offset when it moved it (`scrollIntoView`); nil = the
    /// scroll view owns it.
    var offset: CGPoint?
    var windowed: Bool

    var axes: Axis.Set {
        var a: Axis.Set = []
        if scrollX { a.insert(.horizontal) }
        if scrollY { a.insert(.vertical) }
        return a
    }
}

/// Layer flags (contract §5): `overlay | toast` class, modal (a scrim, the
/// tree beneath inert), dismissible (Escape / scrim / outside press / drag).
extension LayerInfo {
    var paintClass: String { kind == "Toast" || position == "toast" ? "toast" : "overlay" }

    var paintModal: Bool {
        switch kind {
        case "Dialog", "Drawer", "AlertDialog", "Sheet": true
        default: false
        }
    }

    /// The core refuses a dismissal of a non-dismissible layer anyway
    /// (`event(root, "dismiss")`); this only decides whether a scrim press
    /// or a drag is offered.
    @MainActor
    func paintDismissible(_ model: SurfaceModel) -> Bool {
        if kind == "AlertDialog" { return false }
        let owner = model.index(of: owner).flatMap { model.node($0) }
        return owner?.props["dismissible"]?.bool ?? true
    }
}

/// Per-pass derived values, cached per model until the next pass.
@MainActor
final class PaintCache {
    static let shared = PaintCache()

    struct Entry {
        /// The model it was read from (an identifier alone is reused once
        /// a model is freed).
        weak var model: SurfaceModel?
        var pass: Int
        var rtl: Bool
        var reducedMotion: Bool
    }

    private var entries: [ObjectIdentifier: Entry] = [:]

    func entry(_ model: SurfaceModel) -> Entry {
        let key = ObjectIdentifier(model)
        if let e = entries[key], e.model === model, e.pass == model.passCount { return e }
        let settings = model.surface.settings()
        let e = Entry(model: model, pass: model.passCount, rtl: textDirection(locale: settings.locale) == "rtl", reducedMotion: settings.reducedMotion)
        if entries.count > 64 { entries = entries.filter { $0.value.model != nil } }
        entries[key] = e
        return e
    }
}
