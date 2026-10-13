import ExpUI
import SwiftUI

extension View {
    /// EXP-1150: a detail screen's HEADER BAND — the header (tabs, or
    /// nothing) rides the top safe area under the nav bar's title row, and
    /// the bar's material runs up behind both (the host hides the nav bar's
    /// own background and divider), so title and header read as ONE band.
    /// The page scrolls beneath it.
    ///
    /// EXP-1162: the band is the detail chrome's scrim (`headerEdgeChrome`:
    /// the page colour over a blur, a 24pt strip fading it out below).
    /// `onBottom` reports the band's bottom edge in the global space, the line
    /// an issue page's title row collapses the nav-bar title against
    /// (`DetailChrome.isTitleCollapsed`).
    func workHeaderBand<Header: View>(
        onBottom: ((CGFloat) -> Void)? = nil,
        ground: Bool = false,
        @ViewBuilder header: () -> Header
    ) -> some View {
        let header = header()
        return safeAreaInset(edge: .top, spacing: 0) {
            VStack(spacing: 0) {
                header
            }
            .frame(maxWidth: .infinity)
            // EXP-1256: over the run thread, the page's own ground.
            .headerEdgeChrome(ground: ground)
            .onGeometryChange(for: CGFloat.self, of: { $0.frame(in: .global).maxY }) { bottom in
                onBottom?(bottom)
            }
        }
    }
}
