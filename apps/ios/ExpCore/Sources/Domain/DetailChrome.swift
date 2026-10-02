import Foundation

/// EXP-1162 — the Work screen's DETAIL CHROME: the nav-bar title collapse and
/// the scrim strips at the top and bottom edges.
///
/// ONE pure rule, mirrored ×4 (web `lib/detail-chrome.ts`, desktop
/// `domain::detail_chrome`, Android `domain/DetailChrome.kt`) and locked by
/// the contract fixture `domain-contract/fixtures/detail-chrome.json` — same
/// cases, same constants. Points here = px on web/desktop = dp on Android.
public enum DetailChrome {
    /// The collapsed title fades in over this long…
    public static let collapseMs: Double = 160
    /// …while rising this far.
    public static let collapseRise: Double = 4
    /// The strip under the header band that fades scrim + blur to nothing.
    public static let edgeTop: Double = 24
    /// How far above the floating bar's top the bottom strip reaches.
    public static let edgeBottom: Double = 32
    /// The blur behind the scrim.
    public static let edgeBlur: Double = 8
    /// The page background's alpha over that blur.
    public static let scrim: Double = 0.72
    /// A face tab's state dot, this wide…
    public static let faceDot: Double = 6
    /// …trailing the tab's label by this much.
    public static let faceDotGap: Double = 6
    /// The Run tab's agent brand mark (it never draws a dot), this wide…
    public static let faceMark: Double = 14
    /// …leading the tab's label by this much…
    public static let faceMarkGap: Double = 6
    /// …with the needs-input badge this wide at its top trailing corner.
    public static let faceMarkBadge: Double = 6

    /// Whether the header shows the COLLAPSED title (identifier over the
    /// title). A face with no title row of its own is always collapsed; a
    /// title row not measured yet stays expanded; otherwise it BREAKS the
    /// moment the row's bottom edge meets the header band's (same coordinate
    /// space) — a threshold, never a progressive morph.
    public static func isTitleCollapsed(
        hasTitleRow: Bool, titleBottom: Double?, headerBottom: Double
    ) -> Bool {
        guard hasTitleRow else { return true }
        guard let titleBottom else { return false }
        return titleBottom <= headerBottom
    }

    /// FACE DOTS: the header title carries no state dot; the state lives on
    /// the face TABS. The Run tab wears `running` (or `needsInput`, amber,
    /// while the run waits on a person) while its run is LIVE, DRAWN as the
    /// run's agent brand mark (`faceMark`), never a dot; an OPEN pull
    /// request puts `review` on Results, else on Changes. A face not on show
    /// carries no dot, an ended run none.
    public static func faceDots(
        faces: [WorkFaceKind], runLive: Bool, needsInput: Bool, prOpen: Bool
    ) -> [WorkFaceKind: SessionDotTone] {
        var dots: [WorkFaceKind: SessionDotTone] = [:]
        if runLive, faces.contains(.run) {
            dots[.run] = needsInput ? .needsInput : .running
        }
        if prOpen {
            if faces.contains(.results) {
                dots[.results] = .review
            } else if faces.contains(.changes) {
                dots[.changes] = .review
            }
        }
        return dots
    }

    /// What a dotted tab adds to its accessibility label (`Run, running`).
    public static func faceDotSpokenState(_ tone: SessionDotTone) -> String {
        switch tone {
        case .running: "running"
        case .needsInput: "needs input"
        case .review: "pull request open"
        case .done: "done"
        case .muted: "idle"
        }
    }
}
