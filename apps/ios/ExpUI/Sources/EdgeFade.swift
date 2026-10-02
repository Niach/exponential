import ExpCore
import SwiftUI

// EXP-1162: the detail chrome's EDGE STRIPS (`DetailChrome`, the contract
// fixture `detail-chrome.json`). Content scrolls UNDER the header band and the
// floating bottom bar; what sits behind them is the page background at
// `scrim` alpha over a blur, and a thin strip past them fades both to nothing
// — the page receding, no hairline, no hard cut. "Barely noticeable" is the
// goal.
//
// SwiftUI's materials carry their own fixed blur radius, so `edgeBlur` is the
// closest system material (`.ultraThinMaterial`), crossfaded by the mask
// rather than ramped in radius — under a 72% scrim the difference does not
// read.

/// The SOLID scrim: the page colour at `scrim` alpha over the blur. The header
/// band's own background.
public struct EdgeScrim: View {
    let color: Color

    public init(color: Color = GlassTokens.backgroundTop) {
        self.color = color
    }

    public var body: some View {
        ZStack {
            Rectangle().fill(.ultraThinMaterial)
            color.opacity(DetailChrome.scrim)
        }
        .allowsHitTesting(false)
    }
}

/// The FADING strip: `EdgeScrim` masked from full strength at `edge` to
/// nothing at the opposite side. `.top` hangs under a header (full at its top),
/// `.bottom` sits behind a bottom bar (full at the screen's bottom edge).
/// Never takes a touch.
public struct EdgeFade: View {
    public enum Edge: Sendable {
        case top
        case bottom
    }

    let edge: Edge

    public init(edge: Edge) {
        self.edge = edge
    }

    public var body: some View {
        EdgeScrim(color: edge == .top ? GlassTokens.backgroundTop : GlassTokens.backgroundBottom)
            .mask {
                LinearGradient(
                    colors: [.black, .black.opacity(0)],
                    startPoint: edge == .top ? .top : .bottom,
                    endPoint: edge == .top ? .bottom : .top
                )
            }
            .allowsHitTesting(false)
            .accessibilityHidden(true)
    }
}

extension View {
    /// The header band's chrome: `EdgeScrim` behind it (up through the top
    /// safe area) and the `edgeTop` strip hanging below its bottom edge, over
    /// whatever scrolls there. The strip is an overlay, so the band's layout
    /// — and the safe area it hands the content — is untouched.
    public func headerEdgeChrome() -> some View {
        background { EdgeScrim().ignoresSafeArea(edges: .top) }
            .overlay(alignment: .bottom) {
                EdgeFade(edge: .top)
                    .frame(height: DetailChrome.edgeTop)
                    .offset(y: DetailChrome.edgeTop)
            }
    }

    /// The bottom strip behind a floating bottom bar: from the screen's bottom
    /// edge up to `edgeBottom` above the bar's top, fading upwards. A
    /// BACKGROUND of the bar, so it never covers a slot (or an expanded
    /// composer card) and never shifts one. It reaches down through the
    /// container safe area (the home indicator); over a keyboard whatever it
    /// draws past the bar is hidden behind the keys. `visible` hides it
    /// without touching the bar's identity (an expanded composer card wants
    /// none, and its draft must survive the swap).
    public func floatingBarEdge(_ visible: Bool = true) -> some View {
        background(alignment: .bottom) {
            EdgeFade(edge: .bottom)
                .padding(.top, -DetailChrome.edgeBottom)
                .ignoresSafeArea(.container, edges: .bottom)
                .opacity(visible ? 1 : 0)
        }
    }
}
