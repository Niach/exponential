import ExpCore
import ExpUI
import SwiftUI

/// EXP-1150: the Work screen's face TABS — the segmented strip directly under
/// the nav bar, on every face, listing `WorkFaces.availableFaces` in their
/// fixed order (`Issue · Run/Runs · Changes · Results`). It replaced the
/// bottom-right switcher circle (EXP-893). Tapping a segment switches the
/// face; tapping the `Runs` tab while it is ALREADY selected asks for the run
/// menu, which the host anchors under that tab (`runsAnchor`).
///
/// The row is `[strip …][trailing]`: with a trailing control (the Work
/// screen's Merge PR pill) the strip gives up the width it needs and the
/// control sits at the row's end; without one the strip spans the row as
/// before. The strip is drawn only with two or more faces, the row only with
/// a strip or a trailing control.
struct WorkFaceTabs<Trailing: View>: View {
    let faces: [WorkFaceKind]
    let shown: WorkFaceKind
    /// Two or more own runs: the Run tab reads `Runs` and reselecting it opens
    /// the run menu.
    let multipleRuns: Bool
    /// The Run segment's global frame — where the run menu hangs.
    @Binding var runsAnchor: CGRect
    let onSelect: (WorkFaceKind) -> Void
    var onReselectRuns: () -> Void = {}
    /// Whether `trailing` draws anything (a generic view cannot say).
    var showsTrailing: Bool = false
    @ViewBuilder var trailing: () -> Trailing

    private var showsStrip: Bool { faces.count >= 2 }

    var body: some View {
        if showsStrip || showsTrailing {
            HStack(spacing: 8) {
                if showsStrip {
                    strip
                } else {
                    Spacer(minLength: 0)
                }
                if showsTrailing {
                    trailing()
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 8)
        }
    }

    private var strip: some View {
        GlassSegmentedControl(
            options: faces,
            selection: shown,
            label: { WorkFaces.faceLabel($0, multipleRuns: multipleRuns) },
            identifier: { "work-face-\($0.rawValue)" },
            style: .capsule,
            onSelect: tapped
        )
        .onGeometryChange(for: CGRect.self, of: { $0.frame(in: .global) }) { frame in
            runsAnchor = runSegmentFrame(in: frame)
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("work-face-tabs")
    }

    private func tapped(_ face: WorkFaceKind) {
        if face == shown {
            if face == .run, multipleRuns { onReselectRuns() }
            return
        }
        onSelect(face)
    }

    /// The segments tile the strip in equal widths inside its 3pt capsule
    /// padding, so the Run segment's frame follows from the strip's own.
    private func runSegmentFrame(in strip: CGRect) -> CGRect {
        guard let index = faces.firstIndex(of: .run), !faces.isEmpty else { return strip }
        let inset = GlassSegmentedControlTokens.capsulePadding
        let width = (strip.width - inset * 2) / CGFloat(faces.count)
        return CGRect(
            x: strip.minX + inset + width * CGFloat(index),
            y: strip.minY,
            width: width,
            height: strip.height
        )
    }
}

extension WorkFaceTabs where Trailing == EmptyView {
    /// The strip alone (the workflow page's `All`).
    init(
        faces: [WorkFaceKind],
        shown: WorkFaceKind,
        multipleRuns: Bool,
        runsAnchor: Binding<CGRect>,
        onSelect: @escaping (WorkFaceKind) -> Void,
        onReselectRuns: @escaping () -> Void = {}
    ) {
        self.init(
            faces: faces,
            shown: shown,
            multipleRuns: multipleRuns,
            runsAnchor: runsAnchor,
            onSelect: onSelect,
            onReselectRuns: onReselectRuns,
            showsTrailing: false,
            trailing: { EmptyView() }
        )
    }
}

extension View {
    /// EXP-1150: the Work screen's HEADER BAND — the tabs ride the top safe
    /// area under the nav bar's title row, and the bar's material runs up
    /// behind both (the host hides the nav bar's own background and divider),
    /// so title and tabs read as ONE header with ONE hairline under the tabs.
    /// The face scrolls beneath it. Drawn even with no tabs (a lone face), so
    /// the title row keeps its material.
    func workHeaderBand<Header: View>(@ViewBuilder header: () -> Header) -> some View {
        let header = header()
        return safeAreaInset(edge: .top, spacing: 0) {
            VStack(spacing: 0) {
                header
            }
            .frame(maxWidth: .infinity)
            .background(.ultraThinMaterial, ignoresSafeAreaEdges: .top)
            .overlay(alignment: .bottom) {
                Rectangle()
                    .fill(GlassTokens.strokeSection)
                    .frame(height: GlassTokens.hairline)
            }
        }
    }

    /// EXP-1150: a horizontal swipe on the face BODY moves to the neighbouring
    /// face (`WorkFaces.swipeTarget`). A plain `.gesture`, NOT simultaneous:
    /// child gestures keep priority, so a sideways scroller under the finger
    /// (the diff's code) wins, while a vertical-only ScrollView ignores a
    /// clearly horizontal drag and lets it reach here. Decided on the lift,
    /// only for a clearly horizontal fling (≥56pt, more than twice the
    /// vertical travel).
    func workFaceSwipe(
        faces: [WorkFaceKind],
        shown: WorkFaceKind,
        onSwitch: @escaping (WorkFaceKind) -> Void
    ) -> some View {
        gesture(
            DragGesture(minimumDistance: 24).onEnded { value in
                let dx = value.translation.width
                let dy = value.translation.height
                guard abs(dx) >= 56, abs(dx) > 2 * abs(dy) else { return }
                let direction: WorkFaces.SwipeDirection = dx < 0 ? .left : .right
                if let target = WorkFaces.swipeTarget(faces: faces, shown: shown, direction: direction) {
                    onSwitch(target)
                }
            }
        )
    }
}
