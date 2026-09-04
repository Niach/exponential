import Foundation
import XCTest
@testable import ExpCore

// Contract lock (EXP-736; the IssueStatusContractTests playbook): the
// hand-maintained `IssueRelationType` enum, its per-side labels and the six
// picker entries must stay in lockstep with the generated constants in
// DomainContract.generated.swift. The labels are also the event phrases' raw
// material, so a drift here would change activity copy on one client only.
final class IssueRelationContractTests: XCTestCase {
    func testRawValuesMatchGeneratedContract() {
        for value in DomainContract.issueRelationTypeValues {
            let type = IssueRelationType(rawValue: value)
            XCTAssertNotNil(type, "unmapped relation type \(value)")
            XCTAssertEqual(type?.rawValue, value)
        }
        XCTAssertEqual(
            IssueRelationType.allCases.map(\.rawValue),
            DomainContract.issueRelationTypeValues
        )
    }

    func testForwardAndInverseLabelsMatchGeneratedContract() {
        XCTAssertEqual(
            IssueRelationType.allCases.map { $0.label(inverse: false) },
            DomainContract.issueRelationTypeForwardLabels
        )
        XCTAssertEqual(
            IssueRelationType.allCases.map { $0.label(inverse: true) },
            DomainContract.issueRelationTypeInverseLabels
        )
    }

    // The picker's order + wording is a cross-client contract (the reference
    // design): Parent of, Sub-issue of, Blocking, Blocked by, Duplicate of,
    // Related to — and every entry names a type the contract knows.
    func testPickerEntriesAreTheSharedSix() {
        XCTAssertEqual(
            RelationPick.all.map(\.title),
            ["Parent of", "Sub-issue of", "Blocking", "Blocked by", "Duplicate of", "Related to"]
        )
        XCTAssertEqual(
            RelationPick.all.map(\.id),
            ["parent-false", "parent-true", "blocks-false", "blocks-true", "duplicate-false", "related-false"]
        )
        for pick in RelationPick.all {
            XCTAssertTrue(DomainContract.issueRelationTypeValues.contains(pick.type.rawValue))
        }
        // Render order follows the picker; the side with no picker entry
        // ("duplicated by") sorts last rather than crashing.
        XCTAssertEqual(RelationPick.order(type: .parent, inverse: false), 0)
        XCTAssertEqual(RelationPick.order(type: .related, inverse: false), 5)
        XCTAssertEqual(RelationPick.order(type: .duplicate, inverse: true), 6)
    }

    // Byte-identical phrases across the four clients (contract §Events).
    func testEventPhrases() {
        XCTAssertEqual(
            IssueRelationType.eventPhrase(type: .related, inverse: false, identifier: "EXP-12", removed: false),
            "added related issue EXP-12"
        )
        XCTAssertEqual(
            IssueRelationType.eventPhrase(type: .related, inverse: true, identifier: "EXP-12", removed: true),
            "removed related issue EXP-12"
        )
        XCTAssertEqual(
            IssueRelationType.eventPhrase(type: .blocks, inverse: false, identifier: "EXP-3", removed: false),
            "marked as blocks EXP-3"
        )
        XCTAssertEqual(
            IssueRelationType.eventPhrase(type: .blocks, inverse: true, identifier: "EXP-3", removed: true),
            "no longer blocked by EXP-3"
        )
        XCTAssertEqual(
            IssueRelationType.eventPhrase(type: .parent, inverse: true, identifier: "EXP-3", removed: false),
            "marked as sub-issue of EXP-3"
        )
        XCTAssertEqual(
            IssueRelationType.eventPhrase(type: .duplicate, inverse: false, identifier: "EXP-3", removed: false),
            "marked as duplicate of EXP-3"
        )
    }

    // Sources are the contract's, tolerantly decoded on the read side.
    func testSourceValues() {
        XCTAssertEqual(DomainContract.issueRelationSourceValues, ["user", "reference"])
        XCTAssertNil(IssueRelationType.from("mentioned"))
        XCTAssertNil(IssueRelationType.from(nil))
        XCTAssertEqual(IssueRelationType.from("related"), .related)
    }
}
