import SwiftUI
import XCTest
import ExpCore
import ExpUI

// EXP-1029 contract — the shared picker API on iOS. The live cases pin what
// the stubs already do; the skipped ones are the presentation rules and the
// typed-picker gate EXP-1021 implements and un-skips (web, IDE and Android
// carry the same case names).
// The typed pickers are views, so their `items` statics are main-actor
// isolated: the cases that call them have to be too, or the target does not
// build at all.
@MainActor
final class PickerContractTests: XCTestCase {

    func testARowMatchesOnItsKeywordsElseOnItsLabel() {
        XCTAssertEqual(PickerItem(value: "a", label: "Alpha").searchKeywords, ["Alpha"])
        XCTAssertEqual(
            PickerItem(value: "a", label: "Alpha", keywords: ["APP-1"]).searchKeywords, ["APP-1"]
        )
    }

    func testTheTypedPickersMapTheirRowsToItems() {
        XCTAssertEqual(
            BoardPicker<EmptyView>.items([BoardPickerBoard(id: "b", name: "Web", icon: "flag")]).map(\.icon),
            ["flag"]
        )
        XCTAssertEqual(
            IssuePicker<EmptyView>.items([IssuePickerIssue(id: "i", identifier: "APP-1", title: "Fix")]).map(\.label),
            ["APP-1 Fix"]
        )
        XCTAssertEqual(
            AssigneePicker<EmptyView>.items([AssigneePickerMember(id: "u", name: "Ada")], allowsNone: true).map(\.label),
            ["Unassigned", "Ada"]
        )
        // The device set is the registry's append-only `devicePickable` list;
        // `laptop` has been in it since EXP-924 and can never leave.
        XCTAssertTrue(IconPicker.items(for: .device).contains { $0.value == "laptop" && $0.icon == "laptop" })
        XCTAssertFalse(IconPicker.items(for: .board).contains { $0.value == "laptop" })
    }

    func testEveryTypedPickerRendersThroughThePrimitive() throws {
        throw XCTSkip("EXP-1029 contract: EXP-1021 moves the account and icon pickers onto GlassPicker")
    }

    func testAPhoneSheetOfPlainRowsNoCardsInsideTheSheet() throws {
        throw XCTSkip("EXP-1029 contract: EXP-1021 implements the picker sheet")
    }

    func testMultiModeMarksPickedRowsByTheHighlightColourNeverACircle() throws {
        throw XCTSkip("EXP-1029 contract: EXP-1021 implements the picker sheet")
    }

    func testSingleModeClosesOnAPickMultiModeStaysOpen() throws {
        throw XCTSkip("EXP-1029 contract: EXP-1021 implements the picker sheet")
    }

    func testSearchFiltersRowsByLabelAndKeywords() throws {
        throw XCTSkip("EXP-1029 contract: EXP-1021 implements the picker sheet")
    }
}
