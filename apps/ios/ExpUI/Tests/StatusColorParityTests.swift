import Foundation
import SwiftUI
import XCTest
import ExpCore
import ExpUI

// REV2-85: the status/priority COLOR IDENTITY is a cross-client contract, not
// a per-platform taste call. The desktop IDE palette is the source of truth
// (`apps/desktop/crates/domain/src/options.rs` → the same tokens web spells as
// Tailwind classes): cancelled is a MUTED terminal resolution, not an error
// state, not red.
final class StatusColorParityTests: XCTestCase {

    // Cancelled used to be `Semantic.red` here while web/desktop rendered it
    // `text-muted-foreground` — the same issue read as a failure on a phone
    // and as a closed one on the desk.
    func testCancelledIsMutedNotAnErrorState() {
        XCTAssertEqual(IssueStatus.cancelled.color, DesignTokens.Semantic.neutral)
        XCTAssertNotEqual(IssueStatus.cancelled.color, DesignTokens.Semantic.red)
    }

    // The three muted states share one gray; red belongs to `urgent` alone.
    func testTerminalAndParkedStatesShareTheNeutralGray() {
        XCTAssertEqual(IssueStatus.backlog.color, DesignTokens.Semantic.neutral)
        XCTAssertEqual(IssueStatus.duplicate.color, DesignTokens.Semantic.neutral)
    }

    func testActiveStatusAccentsMatchTheSharedTokens() {
        XCTAssertEqual(IssueStatus.inProgress.color, DesignTokens.Semantic.yellow)
        XCTAssertEqual(IssueStatus.inReview.color, DesignTokens.Semantic.green)
        XCTAssertEqual(IssueStatus.done.color, DesignTokens.Semantic.blue)
    }

    func testPriorityAccentsMatchTheSharedTokens() {
        XCTAssertEqual(IssuePriority.urgent.color, DesignTokens.Semantic.red)
        XCTAssertEqual(IssuePriority.high.color, DesignTokens.Semantic.orange)
        XCTAssertEqual(IssuePriority.medium.color, DesignTokens.Semantic.yellow)
        XCTAssertEqual(IssuePriority.low.color, DesignTokens.Semantic.blue)
        XCTAssertEqual(IssuePriority.none.color, DesignTokens.Semantic.neutral)
    }

    // EXP-314: the in_progress / in_review glyphs are the pie-clock pair a
    // 2-started-status team renders — the icon registry re-pointed the
    // semantic concepts, so the enum extension picks them up unchanged.
    func testStartedBuiltinGlyphsAreTheClockPair() {
        XCTAssertEqual(IssueStatus.inProgress.iconName, "progress-2-4")
        XCTAssertEqual(IssueStatus.inReview.iconName, "progress-3-4")
    }

    // EXP-314 rule: a BUILTIN row (or a constructed default) renders today's
    // design token, NOT the synced hex — the tokens are theme-aware and the
    // seed hexes are near-neutral. This keeps builtin rendering byte-identical
    // to before the feature.
    func testBuiltinResolvedStatusesKeepTheTokenColors() {
        for status in IssueStatus.allCases {
            let resolved = IssueStatusResolver.builtinDefault(for: status)
            XCTAssertEqual(resolved.color, status.color, "builtin \(status.rawValue)")
        }
        // …even though the constructed rows DO carry the seeded hex.
        XCTAssertNotNil(IssueStatusResolver.builtinDefault(for: .backlog).colorHex)
    }

    // A CUSTOM row (no builtin key) renders its stored hex through the same
    // parse path labels use, and degrades to the neutral gray when unparsable.
    func testCustomResolvedStatusRendersItsHex() {
        func custom(_ hex: String?) -> ResolvedIssueStatus {
            ResolvedIssueStatus(
                id: "s1", rowId: "s1", name: "Coding", category: .started,
                colorHex: hex, builtinKey: nil, iconName: "progress-2-4"
            )
        }
        XCTAssertEqual(custom("#6366f1").color, Color(hex: "#6366f1"))
        XCTAssertEqual(custom("not-a-color").color, StatusColor.backlog)
        XCTAssertEqual(custom(nil).color, StatusColor.backlog)
    }

    // Pickers speak ONE order everywhere (REV2-85): the contract display
    // order, in_progress-first / urgent-first — the same list the filter and
    // create sheets walk.
    func testPickerOrderIsTheContractDisplayOrder() {
        XCTAssertEqual(
            IssueStatus.displayOrder.map(\.rawValue),
            DomainContract.issueStatusDisplayOrder
        )
        XCTAssertEqual(
            IssuePriority.displayOrder.map(\.rawValue),
            DomainContract.issuePriorityDisplayOrder
        )
    }
}
