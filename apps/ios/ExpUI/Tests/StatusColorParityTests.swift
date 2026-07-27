import Foundation
import SwiftUI
import XCTest
import ExpCore
import ExpUI

// REV2-85: the status/priority COLOR IDENTITY is a cross-client contract, not
// a per-platform taste call. The desktop IDE palette is the source of truth
// (`apps/desktop/crates/domain/src/options.rs` → the same tokens web spells as
// Tailwind classes): cancelled is a MUTED terminal resolution, not an error
// state, and todo carries the brighter near-white foreground tint that
// separates it from backlog's neutral gray.
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

    // Backlog vs todo must be distinguishable without a group header: dashed
    // vs solid glyph AND neutral vs near-white tint (web `text-foreground`,
    // desktop `ColorToken::Foreground`).
    func testTodoIsBrighterThanBacklog() {
        XCTAssertEqual(IssueStatus.todo.color, Zinc._50)
        XCTAssertNotEqual(IssueStatus.todo.color, IssueStatus.backlog.color)
        XCTAssertNotEqual(IssueStatus.todo.iconName, IssueStatus.backlog.iconName)
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
