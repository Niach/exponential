import Foundation

/// EXP-656: what a scroll-geometry sample does to the agent feed's follow pin.
///
/// The pin is derived geometry ("the content's bottom edge is within slack of
/// the viewport's"), and geometry moves for two very different reasons: the
/// user scrolled, or the CONTENT changed size under a viewer who never
/// touched the screen. Only the first is a follow decision. Content SHRINKING
/// under a stationary reader — a staged replay committing a shorter history,
/// a group collapsing — pulls the bottom edge up into slack range and used to
/// read as "the reader is at the bottom again", which re-armed follow and
/// scrolled a reader out of the middle of a plan.
///
/// Pure, so the rule is testable without a scroll view (AgentSessionView's
/// FollowPinTracker feeds it the two samples' deltas). Distances are Double —
/// ExpCore is Foundation-only and the view converts its CGFloats.
public enum FeedFollowAction: Equatable, Sendable {
    /// Follow again: the reader is at the bottom.
    case rearm
    /// The reader scrolled away — stop following.
    case unpin
    /// Leave the pin exactly as it is.
    case hold
}

public enum FeedFollowPolicy {
    /// - Parameters:
    ///   - pinned: this sample's geometry verdict (bottom edge within slack).
    ///   - atBottom: the follow pin as it stands.
    ///   - userScrolling: a drag or its momentum is driving the offset.
    ///   - offsetDelta: how the content offset moved since the previous
    ///     sample; negative = towards the top of the feed.
    ///   - heightDelta: how the content height moved; negative = the feed got
    ///     shorter under the viewer.
    public static func decide(
        pinned: Bool, atBottom: Bool, userScrolling: Bool,
        offsetDelta: Double, heightDelta: Double
    ) -> FeedFollowAction {
        if pinned {
            guard !atBottom else { return .hold }
            // A pin nobody scrolled into: the content shrank and the bottom
            // edge came to the reader, not the other way round. Re-arming here
            // is exactly the yank EXP-656 is about.
            if heightDelta < 0, !userScrolling, offsetDelta == 0 { return .hold }
            // Reaching the bottom always re-arms follow (Android parity).
            return .rearm
        }
        // Leaving the bottom is the USER's alone: content growth un-pins the
        // geometry with no gesture, and the growth chaser re-pins instead
        // (EXP-272/EXP-306). Hence both a live gesture and an upward offset.
        if userScrolling, atBottom, offsetDelta < 0 { return .unpin }
        return .hold
    }
}
