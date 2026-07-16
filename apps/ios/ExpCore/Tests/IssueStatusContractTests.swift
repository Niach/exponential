import Foundation
import XCTest
@testable import ExpCore

// Contract lock (mirrors desktop's enums.rs tests): the hand-maintained
// `IssueStatus` enum must stay in lockstep with the generated constants in
// DomainContract.generated.swift — a contract regen that adds/renames a value
// or reorders the display order fails here instead of drifting silently.
final class IssueStatusContractTests: XCTestCase {
    // Every canonical wire value maps to a case that round-trips back to the
    // same raw value, and the enum defines no extra cases beyond the contract.
    func testIssueStatusRawValuesMatchGeneratedContract() {
        for value in DomainContract.issueStatusValues {
            let status = IssueStatus(rawValue: value)
            XCTAssertNotNil(status, "unmapped status \(value)")
            XCTAssertEqual(status?.rawValue, value)
        }
        XCTAssertEqual(
            IssueStatus.allCases.map(\.rawValue),
            DomainContract.issueStatusValues
        )
    }

    // Display order matches the generated contract exactly, element for element.
    func testIssueStatusDisplayOrderMatchesGeneratedContract() {
        XCTAssertEqual(
            IssueStatus.displayOrder.map(\.rawValue),
            DomainContract.issueStatusDisplayOrder
        )
    }

    // The tolerant wire decoder: unknown/missing values fall back to .backlog
    // (iOS's analog of desktop's Unknown), known values map exactly.
    func testFromWireFallsBackToBacklog() {
        XCTAssertEqual(IssueStatus.from("in_review"), .inReview)
        XCTAssertEqual(IssueStatus.from("triaged"), .backlog)
        XCTAssertEqual(IssueStatus.from(nil), .backlog)
    }
}
