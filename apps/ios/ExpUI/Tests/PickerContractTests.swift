import SwiftUI
import XCTest
import ExpCore
import ExpUI

// EXP-1029 contract — the shared picker API on iOS. The live cases pin what
// the stubs already do; the skipped ones are the presentation rules and the
// typed-picker gate EXP-1021 implements and un-skips (web, IDE and Android
// carry the same case names).
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
        XCTAssertEqual(IconPicker.items(for: .device).count, DomainContract.deviceIconValues.count)
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
