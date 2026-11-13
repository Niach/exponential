import Foundation

extension WaitingCards {
    /// waiting-cards.json DEEP LINKS: THE pending deep link a session view
    /// holds until it settles. Every layout / arrival the view hands it what
    /// it sees (`Frame`); `step` folds that through `deepLinkStep` into what
    /// the view does next. Pure, so the whole loop (page → reveal → scroll →
    /// ring) is testable without a scroll view.
    public struct DeepLink: Equatable, Sendable {
        /// The message the link names.
        public let target: String
        /// When the link was taken (ms, the caller's clock).
        public let takenAtMs: Double
        /// An older page is in flight (`page` until `pageLanded`).
        public private(set) var paging = false
        /// It rang or went missing: nothing more to do.
        public private(set) var settled = false
        /// The row was scrolled to by estimate once (a lazy row not realised).
        private var revealed = false
        /// Scrolls issued, and the row's distance from the centre at the last one.
        private var scrolls = 0
        private var lastDistance: Double?

        /// Within this many points of the viewport's centre = centred.
        public static let centreSlack: Double = 2
        /// After this many scrolls that left the row where it was, the list
        /// cannot go further toward it.
        public static let stallScrolls = 3

        public init(target: String, takenAtMs: Double) {
            self.target = target
            self.takenAtMs = takenAtMs
        }

        /// What the view sees this frame.
        public struct Frame: Equatable, Sendable {
            /// The target is among the rows the view draws.
            public var found: Bool
            /// The row's centre minus the viewport's centre, as last laid
            /// out; nil = not laid out (a lazy row not realised yet).
            public var distance: Double?
            /// The list cannot scroll further toward the row (top or bottom edge).
            public var atEdge: Bool
            /// Older rows can still page in.
            public var more: Bool

            public init(found: Bool, distance: Double? = nil, atEdge: Bool = false, more: Bool) {
                self.found = found
                self.distance = distance
                self.atEdge = atEdge
                self.more = more
            }
        }

        public enum Action: Equatable, Sendable {
            /// Nothing this frame (wait for the next layout / arrival).
            case none
            /// The row is loaded but not realised: scroll to it by estimate
            /// so it lays out (then `scroll` centres it exactly).
            case reveal
            /// Scroll the row to the viewport's centre.
            case scroll
            /// Page ONE older page in (the transcript's Load earlier).
            case page
            /// THE highlight ring (`highlightMs`); the link settles.
            case ring
            /// Not there: the link settles, the chat stays where it is.
            case missing
        }

        public mutating func step(_ frame: Frame, nowMs: Double) -> Action {
            guard !settled else { return .none }
            let distance = frame.distance
            let stalled = scrolls >= Self.stallScrolls
                && distance != nil && lastDistance != nil
                && abs(distance! - lastDistance!) < 0.5
            let centred = distance.map { abs($0) <= Self.centreSlack } == true || frame.atEdge || stalled
            let step = WaitingCards.deepLinkStep(DeepLinkInput(
                found: frame.found, laidOut: distance != nil, centred: centred,
                paging: paging, more: frame.more, elapsedMs: nowMs - takenAtMs
            ))
            switch step {
            case .ring:
                settled = true
                return .ring
            case .missing:
                settled = true
                return .missing
            case .scroll:
                scrolls += 1
                lastDistance = distance
                return .scroll
            case .page:
                paging = true
                return .page
            case .wait:
                guard frame.found, distance == nil, !revealed else { return .none }
                revealed = true
                return .reveal
            }
        }

        /// The page asked for landed (or could not load): step again.
        public mutating func pageLanded() {
            paging = false
            revealed = false
        }
    }
}
