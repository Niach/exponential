import Foundation
import XCTest
@testable import ExpCore

// EXP-273: the "Create action" builtin declares an OPTIONAL `icon` input whose
// value is a curated registry name. It must be a supported type (never the
// "needs a newer app version" block), must default to none, and a picked glyph
// must reach the wire as the bare registry name — matching the web helper
// apps/web/src/lib/action-inputs.ts.
final class ActionInputValuesTests: XCTestCase {
    private let createActionInputs = ActionDto.builtinCreateAction(teamId: "t-1").inputs ?? []

    private func iconInput(required: Bool = false) -> ActionInputDto {
        ActionInputDto(key: "icon", label: "Icon", type: "icon", required: required)
    }

    func testIconIsASupportedInputType() {
        XCTAssertFalse(ActionInputValues.hasUnsupportedType([iconInput()]))
        XCTAssertTrue(DomainContract.actionInputTypeValues.contains("icon"))
    }

    func testUnknownInputTypesStayBlocked() {
        let future = ActionInputDto(key: "k", label: "Label", type: "hologram", required: false)
        XCTAssertTrue(ActionInputValues.hasUnsupportedType([future]))
        XCTAssertTrue(ActionInputValues.hasUnsupportedType(createActionInputs + [future]))
    }

    // The builtin's icon input is optional, so an untouched picker must not
    // block the run — the bug was a whole builtin that could not be launched.
    func testTheCreateActionBuiltinRunsWithNoIconPicked() {
        XCTAssertFalse(ActionInputValues.hasUnsupportedType(createActionInputs))
        XCTAssertTrue(
            ActionInputValues.requiredFilled(
                createActionInputs,
                values: ["description": "Ship a changelog action"]
            )
        )
        XCTAssertEqual(
            ActionInputValues.wireValues(
                createActionInputs,
                values: ["description": "Ship a changelog action"]
            ),
            ["description": "Ship a changelog action"]
        )
    }

    func testAPickedIconSubmitsItsRegistryName() {
        let values = ActionInputValues.wireValues(
            createActionInputs,
            values: ["description": "Ship it", "icon": "rocket", "repo": "repo-uuid"]
        )
        XCTAssertEqual(values["icon"], "rocket")
        XCTAssertEqual(values, ["description": "Ship it", "icon": "rocket", "repo": "repo-uuid"])
        // Whatever the grid emits is a storable board icon.
        XCTAssertTrue(DomainContract.boardIconValues.contains("rocket"))
    }

    // Clearing the picker writes "" (non-nil, so it is never re-seeded) — the
    // wire map must drop it rather than send an empty glyph name.
    func testAClearedIconIsDroppedFromTheWireValues() {
        let values = ActionInputValues.wireValues(
            createActionInputs,
            values: ["description": "Ship it", "icon": ""]
        )
        XCTAssertNil(values["icon"])
        XCTAssertEqual(values, ["description": "Ship it"])
    }

    func testARequiredIconMustBePicked() {
        XCTAssertFalse(ActionInputValues.requiredFilled([iconInput(required: true)], values: [:]))
        XCTAssertFalse(
            ActionInputValues.requiredFilled([iconInput(required: true)], values: ["icon": ""])
        )
        XCTAssertTrue(
            ActionInputValues.requiredFilled([iconInput(required: true)], values: ["icon": "bug"])
        )
    }

    // Only `text` is trimmed; ids and glyph names go through verbatim.
    func testTextIsTrimmedAndBlanksAreDropped() {
        let defs = [
            ActionInputDto(key: "note", label: "Note", type: "text", required: false),
            ActionInputDto(key: "blank", label: "Blank", type: "text", required: false),
        ]
        XCTAssertEqual(
            ActionInputValues.wireValues(defs, values: ["note": "  hi  ", "blank": "   "]),
            ["note": "hi"]
        )
        XCTAssertTrue(ActionInputValues.requiredFilled(defs, values: [:]))
    }

    func testTextLengthCapAppliesOnlyToTextInputs() {
        let text = ActionInputDto(key: "note", label: "Note", type: "text", required: false)
        let tooLong = String(repeating: "a", count: DomainContract.actionInputTextMax + 1)
        XCTAssertFalse(ActionInputValues.textsWithinLimit([text], values: ["note": tooLong]))
        XCTAssertTrue(
            ActionInputValues.textsWithinLimit(
                [text],
                values: ["note": String(repeating: "a", count: DomainContract.actionInputTextMax)]
            )
        )
        XCTAssertTrue(ActionInputValues.textsWithinLimit([iconInput()], values: ["icon": tooLong]))
    }

    // EXP-530: `textarea` is a supported type that shares every `text` rule —
    // trim, blank-drop, and the length cap.
    func testTextareaSharesTheTextRules() {
        let textarea = ActionInputDto(key: "notes", label: "Notes", type: "textarea", required: false)
        XCTAssertFalse(ActionInputValues.hasUnsupportedType([textarea]))
        XCTAssertTrue(DomainContract.actionInputTypeValues.contains("textarea"))

        XCTAssertEqual(
            ActionInputValues.wireValues([textarea], values: ["notes": "  hi\nthere  "]),
            ["notes": "hi\nthere"]
        )
        XCTAssertEqual(ActionInputValues.wireValues([textarea], values: ["notes": "   "]), [:])

        let tooLong = String(repeating: "a", count: DomainContract.actionInputTextMax + 1)
        XCTAssertFalse(ActionInputValues.textsWithinLimit([textarea], values: ["notes": tooLong]))
        XCTAssertTrue(
            ActionInputValues.textsWithinLimit(
                [textarea],
                values: ["notes": String(repeating: "a", count: DomainContract.actionInputTextMax)]
            )
        )
    }
}
