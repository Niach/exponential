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
/// EXP-1154: the strip alone, centred — the Merge PR pill that trailed it
/// (EXP-1150) went back to the floating bottom bar. Drawn only with two or
/// more faces.
///
/// EXP-1152: the Changes tab wears the diff's `+N −M` once its counts are
/// known (`WorkFaces.changesFaceCounts`, the desktop's `FaceToggle::diff`),
/// the word until then; the body under the strip is `WorkFacePager`.
struct WorkFaceTabs: View {
    let faces: [WorkFaceKind]
    let shown: WorkFaceKind
    /// Two or more own runs: the Run tab reads `Runs` and reselecting it opens
    /// the run menu.
    let multipleRuns: Bool
    /// EXP-1152: the Changes tab's counts — nil keeps the word `Changes`.
    var changesCounts: WorkFaces.ChangesFaceCounts? = nil
    /// EXP-1162: the face tabs' tones (`DetailChrome.faceDots`) — the state
    /// the header title no longer carries. Run's is drawn as its agent mark.
    var dots: [WorkFaceKind: SessionDotTone] = [:]
    /// The shown run's coding agent (contract `codingAgent`): the Run tab's
    /// tone is drawn as THIS brand mark, never a dot. Nil = the agents glyph.
    var runAgent: String? = nil
    /// EXP-1184: what the shown run is doing (the `session-display.json`
    /// rule, fed the viewer's own busy signal): the Run tab wears the working
    /// mark, or the brand mark with a state badge. Nil = the bare mark (no
    /// live run, or a paused one).
    var runState: CodingSessionDisplayState? = nil
    /// The Run segment's global frame — where the run menu hangs.
    @Binding var runsAnchor: CGRect
    let onSelect: (WorkFaceKind) -> Void
    var onReselectRuns: () -> Void = {}

    private var showsStrip: Bool { faces.count >= 2 }

    var body: some View {
        if showsStrip {
            strip
                .frame(maxWidth: .infinity)
                .padding(.horizontal, 16)
                .padding(.vertical, 8)
        }
    }

    private var strip: some View {
        GlassSegmentedControl(
            options: faces,
            selection: shown,
            label: segmentLabel,
            identifier: { "work-face-\($0.rawValue)" },
            content: segmentContent,
            leading: segmentMark,
            leadingGap: DetailChrome.faceMarkGap,
            accessory: segmentDot,
            accessoryGap: DetailChrome.faceDotGap,
            spokenLabel: spokenLabel,
            style: .capsule,
            onSelect: tapped
        )
        .onGeometryChange(for: CGRect.self, of: { $0.frame(in: .global) }) { frame in
            runsAnchor = runSegmentFrame(in: frame)
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("work-face-tabs")
    }

    /// The segment's words — and its accessibility label: the counts read as
    /// `+12 −2`, the same string web names the segment by.
    private func segmentLabel(_ face: WorkFaceKind) -> String {
        if face == .changes, let changesCounts {
            return WorkFaces.changesFaceText(changesCounts)
        }
        return WorkFaces.faceLabel(face, multipleRuns: multipleRuns)
    }

    /// EXP-1152: the counts in the diff's own green and red, mono at the
    /// strip's `.subheadline` rung — the same `+a −b` every diff card wears.
    private func segmentContent(_ face: WorkFaceKind) -> AnyView? {
        guard face == .changes, let changesCounts else { return nil }
        return AnyView(
            DiffCountsLabel(
                additions: changesCounts.additions,
                deletions: changesCounts.deletions,
                font: .subheadline.monospaced().weight(.medium)
            )
        )
    }

    /// EXP-1162/1184: the Run tab's tone as the run's agent mark, leading
    /// the label — the working mark while the agent works, else the brand
    /// mark with the state's badge (`AgentRunMark`).
    private func segmentMark(_ face: WorkFaceKind) -> AnyView? {
        guard face == .run, let tone = dots[face] else { return nil }
        let state = runState ?? (tone == .needsInput ? .needsInput : nil)
        return AnyView(
            AgentRunMark(agent: runAgent, state: state, badgeSize: DetailChrome.faceMarkBadge)
                .frame(width: DetailChrome.faceMark, height: DetailChrome.faceMark)
                .accessibilityHidden(true)
        )
    }

    /// EXP-1162: the tab's trailing state dot, in the session-dot colours —
    /// every tab but Run, which wears its mark instead.
    private func segmentDot(_ face: WorkFaceKind) -> AnyView? {
        guard face != .run, let tone = dots[face] else { return nil }
        return AnyView(
            SessionStateDot(tone: tone, size: DetailChrome.faceDot)
                .accessibilityHidden(true)
        )
    }

    /// A dotted tab says its state: `Run, running`, `Results, pull request
    /// open`.
    private func spokenLabel(_ face: WorkFaceKind) -> String? {
        guard let tone = dots[face] else { return nil }
        if face == .run, let runState {
            return "\(segmentLabel(face)), \(Self.spokenState(runState))"
        }
        return "\(segmentLabel(face)), \(DetailChrome.faceDotSpokenState(tone))"
    }

    /// EXP-1184: the Run tab's spoken state (web `RUN_STATE_LABEL`).
    private static func spokenState(_ state: CodingSessionDisplayState) -> String {
        switch state {
        case .working: "working"
        case .needsInput: "needs input"
        case .review: "pull request open"
        case .done: "done"
        }
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

extension View {
    /// EXP-1150: the Work screen's HEADER BAND — the tabs ride the top safe
    /// area under the nav bar's title row, and the bar's material runs up
    /// behind both (the host hides the nav bar's own background and divider),
    /// so title and tabs read as ONE header.
    /// The face scrolls beneath it. Drawn even with no tabs (a lone face), so
    /// the title row keeps its material.
    ///
    /// EXP-1162: no hairline any more — the band is the detail chrome's scrim
    /// (`headerEdgeChrome`: the page colour over a blur, a 24pt strip fading
    /// it out below). `onBottom` reports the band's bottom edge in the global
    /// space, the line the Issue face's title row collapses the nav-bar title
    /// against (`DetailChrome.isTitleCollapsed`).
    func workHeaderBand<Header: View>(
        onBottom: ((CGFloat) -> Void)? = nil,
        @ViewBuilder header: () -> Header
    ) -> some View {
        let header = header()
        return safeAreaInset(edge: .top, spacing: 0) {
            VStack(spacing: 0) {
                header
            }
            .frame(maxWidth: .infinity)
            .headerEdgeChrome()
            .onGeometryChange(for: CGFloat.self, of: { $0.frame(in: .global).maxY }) { bottom in
                onBottom?(bottom)
            }
        }
    }
}

/// EXP-1152: the faces as PAGES that follow the finger (`FacePager`), one
/// page per face in the strip's order. It replaced EXP-1150's
/// `workFaceSwipe`, a `DragGesture` decided on the lift that swapped the face
/// INSTANTLY and fought the faces' own ScrollViews for the touch ("buggy and
/// hard to drag").
///
/// `selection` is the screen's face: a tab tap moves it (animated, so the
/// pages slide), and a drag onto a page writes it back through the host's own
/// switch path. A `selection` missing from `faces` (a requested face that has
/// not synced yet) still gets its page, in its fixed place — the pager must
/// never snap it onto a neighbour and lose the request.
struct WorkFacePager<Page: View>: View {
    let faces: [WorkFaceKind]
    @Binding var selection: WorkFaceKind
    @ViewBuilder let page: (WorkFaceKind) -> Page

    private var pages: [WorkFaceKind] {
        WorkFaceKind.allCases.filter { faces.contains($0) || $0 == selection }
    }

    var body: some View {
        FacePager(pages: pages, selection: $selection, page: page)
    }
}

/// The pager under a tab strip: a horizontal PAGING scroll view, one
/// full-size page per tag, mounted as it comes into reach. The scroll view
/// arbitrates a nested vertical feed and a sideways code scroller itself.
///
/// EXP-1160: deliberately NOT `TabView` in its `.page` style, which this was
/// until then. On the page a screen opened on, the first `.sheet` presented
/// from inside the page was presented TWICE (UIKit: "already presenting") and
/// both were torn down again — the first tap on a property chip did nothing —
/// and its pages lost the safe area (title jammed under the header band, the
/// bottom bar over the home indicator). A scroll view's pages are plain
/// children: sheets, alerts and `safeAreaInset` bars behave as on any screen,
/// so a page presents its own sheets like every other view does.
struct FacePager<Tag: Hashable, Page: View>: View {
    let pages: [Tag]
    @Binding var selection: Tag
    @ViewBuilder let page: (Tag) -> Page

    var body: some View {
        // The scroll view spans the WHOLE screen and every page gets the safe
        // area back as its own (the header band above, the home indicator or
        // the keyboard below, the side insets in landscape). Left to the
        // scroll view, the cross-axis safe area is OS-dependent (iOS 18 sizes
        // a page to the full container and pushes its bottom bar off screen)
        // and the neighbour page shows through the side insets.
        GeometryReader { geometry in
            let insets = geometry.safeAreaInsets
            ScrollViewReader { scroller in
                ScrollView(.horizontal) {
                    LazyHStack(spacing: 0) {
                        ForEach(pages, id: \.self) { tag in
                            page(tag)
                                .frame(maxWidth: .infinity, maxHeight: .infinity)
                                .safeAreaPadding(insets)
                                .frame(
                                    width: geometry.size.width + insets.leading + insets.trailing,
                                    height: geometry.size.height + insets.top + insets.bottom
                                )
                        }
                    }
                    .scrollTargetLayout()
                }
                .scrollTargetBehavior(.paging)
                .scrollPosition(id: Binding(
                    get: { selection },
                    set: { if let next = $0, next != selection { selection = next } }
                ))
                .scrollIndicators(.hidden)
                .ignoresSafeArea()
                // A page that arrives BEFORE the shown one (a run's issue
                // syncing in) shifts the row under the viewport: land on the
                // selection again, unanimated.
                .onChange(of: pages) {
                    scroller.scrollTo(selection, anchor: .leading)
                }
                // And a new width (rotation, an iPad split) moves every page's
                // origin while the offset stays put — once the pages took
                // their new size, not before.
                .onChange(of: geometry.size.width) {
                    DispatchQueue.main.async {
                        scroller.scrollTo(selection, anchor: .leading)
                    }
                }
            }
        }
    }
}
