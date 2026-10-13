import Foundation
import XCTest
@testable import ExpCore

// The typed action inputs (EXP-257; EXP-1291: repo | board | pr | issue picks).
// A row declaring anything else — the retired free-text types, a future type —
// is blocked, required picks must be filled, and only declared, non-blank
// values reach the wire (web `lib/action-inputs.ts`).
final class ActionInputValuesTests: XCTestCase {
    private func input(_ key: String, _ type: String, required: Bool = false) -> ActionInputDto {
        ActionInputDto(key: key, label: key.capitalized, type: type, required: required)
    }

    func testOnlyThePickTypesAreSupported() {
        XCTAssertEqual(DomainContract.actionInputTypeValues, ["repo", "board", "pr", "issue"])
        for type in DomainContract.actionInputTypeValues {
            XCTAssertFalse(ActionInputValues.hasUnsupportedType([input("k", type)]), "\(type) must be supported")
        }
    }

    func testUnknownAndRetiredInputTypesStayBlocked() {
        XCTAssertTrue(ActionInputValues.hasUnsupportedType([input("k", "hologram")]))
        XCTAssertTrue(ActionInputValues.hasUnsupportedType([input("repo", "repo"), input("k", "hologram")]))
        XCTAssertTrue(ActionInputValues.hasUnsupportedType([input("note", "text")]))
        XCTAssertTrue(ActionInputValues.hasUnsupportedType([input("notes", "textarea")]))
        XCTAssertTrue(ActionInputValues.hasUnsupportedType([input("icon", "icon")]))
    }

    // An untouched composer with only optional inputs must not block the run;
    // the wire map can legitimately be EMPTY.
    func testOptionalInputsRunWithNothingPicked() {
        let defs = [input("repo", "repo"), input("board", "board")]
        XCTAssertTrue(ActionInputValues.requiredFilled(defs, values: [:]))
        XCTAssertEqual(ActionInputValues.wireValues(defs, values: [:]), [:])
    }

    func testARequiredPickMustBeFilled() {
        let pr = input("pr", "pr", required: true)
        XCTAssertFalse(ActionInputValues.requiredFilled([pr], values: [:]))
        XCTAssertFalse(ActionInputValues.requiredFilled([pr], values: ["pr": ""]))
        // A required pick with only whitespace is still unset.
        XCTAssertFalse(ActionInputValues.requiredFilled([pr], values: ["pr": "   "]))
        XCTAssertTrue(ActionInputValues.requiredFilled([pr], values: ["pr": "pr-1"]))
    }

    // Clearing a picker writes "" (non-nil, so it is never re-seeded) — the
    // wire map drops it; keys the action does not declare never reach it.
    func testBlankAndUndeclaredValuesAreDropped() {
        let defs = [input("repo", "repo"), input("issue", "issue")]
        XCTAssertEqual(
            ActionInputValues.wireValues(defs, values: ["repo": "", "issue": "i-1", "pr": "pr-1"]),
            ["issue": "i-1"]
        )
    }
}
