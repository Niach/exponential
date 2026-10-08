import SwiftUI
#if canImport(AppKit) && !targetEnvironment(macCatalyst)
import AppKit
#endif

/// The paint-only keys of a node's box (contract §2, §9 SwiftUI 2):
/// `transform` (translate / scale / rotate about the centre, AFTER layout:
/// the frame never moves, hit-testing follows the paint like the web),
/// `visibility: hidden` (the box keeps its place, paints nothing, takes no
/// press and leaves the a11y tree) and `pointerEvents: none` (presses pass
/// through). The rtl mirror of a directional Icon is the OUTERMOST
/// transform (contract §4).
struct PaintOnly: ViewModifier {
    let style: PaintStyle
    let animation: Animation?
    let mirror: Bool

    func body(content: Content) -> some View {
        let t = style.transform
        content
            .animation(animation) { v in
                v
                    .scaleEffect(t.scale, anchor: .center)
                    .rotationEffect(.degrees(t.rotate), anchor: .center)
                    .offset(x: t.tx, y: t.ty)
                    .opacity(style.invisible ? 0 : 1)
            }
            .mirroredForRTL(mirror)
            .allowsHitTesting(!style.pointerNone && !style.invisible)
            .accessibilityHidden(style.invisible)
    }
}

/// Overflow clipping per axis (`overflowX` / `overflowY`): both axes clip
/// to the rounded box; one axis clips to a band unbounded along the other
/// (no radius, like gpui's `overflow_x_hidden`).
struct ClipModifier: ViewModifier {
    let style: PaintStyle
    let size: CGSize

    func body(content: Content) -> some View {
        if style.clipX && style.clipY {
            content.clipShape(style.shape(size))
        } else if style.clipX || style.clipY {
            content.clipShape(AxisClip(x: style.clipX, y: style.clipY))
        } else {
            content
        }
    }
}

/// A rectangle clipping one axis only.
struct AxisClip: Shape {
    let x: Bool
    let y: Bool

    func path(in rect: CGRect) -> Path {
        let far: CGFloat = 1e5
        return Path(CGRect(x: x ? rect.minX : rect.minX - far, y: y ? rect.minY : rect.minY - far, width: x ? rect.width : rect.width + 2 * far, height: y ? rect.height : rect.height + 2 * far))
    }
}

/// The keyboard focus ring (`:focus-visible` only, contract §6): the
/// recipe `ring` colour, 2 pt outside the box, following its corners.
struct FocusRing: View {
    let style: PaintStyle
    let size: CGSize
    let color: Color

    var body: some View {
        BoxShape(radii: style.clampedRadii(size).map { $0 + 2 })
            .strokeBorder(color, lineWidth: 2)
            .padding(-2)
            .allowsHitTesting(false)
            .accessibilityHidden(true)
    }
}

/// Pointer hover → the node's `hover` state (macOS, iPadOS pointer); the
/// core restyles `:hover` (contract §2).
struct HoverReporting: ViewModifier {
    let id: String
    let model: SurfaceModel
    let on: Bool

    func body(content: Content) -> some View {
        if on {
            content.onHover { model.hover(id, $0) }
        } else {
            content
        }
    }
}

/// `cursor` (contract §2): a hint pointer platforms honour; touch ignores
/// it. macOS pushes the matching `NSCursor` while the pointer is over the
/// box.
struct CursorModifier: ViewModifier {
    let cursor: String?

    func body(content: Content) -> some View {
        #if canImport(AppKit) && !targetEnvironment(macCatalyst)
        if let c = cursor.flatMap(Self.nsCursor) {
            content.onHover { inside in
                if inside { c.push() } else { NSCursor.pop() }
            }
        } else {
            content
        }
        #else
        content
        #endif
    }

    #if canImport(AppKit) && !targetEnvironment(macCatalyst)
    /// `cursor` → AppKit (the gpui painter's `cursor_of`).
    static func nsCursor(_ name: String) -> NSCursor? {
        switch name {
        case "pointer": .pointingHand
        case "text": .iBeam
        case "not-allowed": .operationNotAllowed
        case "grab": .openHand
        case "grabbing", "move": .closedHand
        case "col-resize": .resizeLeftRight
        case "row-resize": .resizeUpDown
        case "default": .arrow
        default: nil
        }
    }
    #endif
}

/// The CSS `cubic-bezier(x1, y1, x2, y2)` at progress `x` (0…1), the
/// curve SwiftUI's `.timingCurve` runs; tests and painters that step a
/// curve themselves use it.
func cubicBezier(_ e: [Double], _ x: Double) -> Double {
    let x = min(1, max(0, x))
    guard e.count == 4 else { return x }
    let (x1, y1, x2, y2) = (e[0], e[1], e[2], e[3])
    func bez(_ t: Double, _ p1: Double, _ p2: Double) -> Double {
        let u = 1 - t
        return 3 * u * u * t * p1 + 3 * u * t * t * p2 + t * t * t
    }
    // Bisection on x(t) = x (monotone for x1, x2 in 0…1).
    var lo = 0.0, hi = 1.0, t = x
    for _ in 0..<40 {
        let v = bez(t, x1, x2)
        if abs(v - x) < 1e-6 { break }
        if v < x { lo = t } else { hi = t }
        t = (lo + hi) / 2
    }
    return bez(t, y1, y2)
}
