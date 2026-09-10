import Foundation
import XCTest
@testable import ExpCore

// EXP-273: the "Create action" builtin declares an OPTIONAL `icon` input whose
// value is a curated registry name. It must be a supported type (never the
// "needs a newer app version" block), must default to none, and a picked glyph
// must reach the wire as the bare registry name — matching the web helper
// apps/web/src/lib/action-inputs.ts.
//
// EXP-825: the free-text `text` / `textarea` types are RETIRED — whatever the
// requester types is the start's `prompt` — so a row still declaring one is
// blocked exactly like an unknown future type, and only pick values ride
// `inputs`.
final class ActionInputValuesTests: XCTestCase {
    private let createActionInputs = ActionDto.builtinCreateAction(teamId: "t-1").inputs ?? []

    private func iconInput(required: Bool = false) -> ActionInputDto {
        ActionInputDto(key: "icon", label: "Icon", type: "icon", required: required)
    }

    func testIconIsASupportedInputType() {
        XCTAssertFalse(ActionInputValues.hasUnsupportedType([iconInput()]))
        XCTAssertTrue(DomainContract.actionInputTypeValues.contains("icon"))
    }

    func testOnlyThePickTypesAreSupported() {
        XCTAssertEqual(DomainContract.actionInputTypeValues, ["repo", "board", "pr", "icon"])
        for type in DomainContract.actionInputTypeValues {
            XCTAssertFalse(
                ActionInputValues.hasUnsupportedType([
                    ActionInputDto(key: "k", label: "Label", type: type, required: false),
                ]),
                "\(type) must be supported"
            )
        }
    }

    func testUnknownInputTypesStayBlocked() {
        let future = ActionInputDto(key: "k", label: "Label", type: "hologram", required: false)
        XCTAssertTrue(ActionInputValues.hasUnsupportedType([future]))
        XCTAssertTrue(ActionInputValues.hasUnsupportedType(createActionInputs + [future]))
    }

    // EXP-825: a stale synced row that still declares free text (the server
    // migration strips them, but a row can be older than this build's view of
    // it) is BLOCKED, not rendered as a field the run could half-fill.
    func testRetiredFreeTextTypesAreBlocked() {
        let text = ActionInputDto(key: "note", label: "Note", type: "text", required: false)
        let textarea = ActionInputDto(key: "notes", label: "Notes", type: "textarea", required: false)
        XCTAssertTrue(ActionInputValues.hasUnsupportedType([text]))
        XCTAssertTrue(ActionInputValues.hasUnsupportedType([textarea]))
        XCTAssertFalse(DomainContract.actionInputTypeValues.contains("text"))
        XCTAssertFalse(DomainContract.actionInputTypeValues.contains("textarea"))
    }

    // Both of the builtin's inputs are optional, so an untouched composer
    // must not block the run — the request text is the `prompt`, not an
    // input, so the wire map can legitimately be EMPTY.
    func testTheCreateActionBuiltinRunsWithNothingPicked() {
        XCTAssertFalse(ActionInputValues.hasUnsupportedType(createActionInputs))
        XCTAssertTrue(ActionInputValues.requiredFilled(createActionInputs, values: [:]))
        XCTAssertEqual(ActionInputValues.wireValues(createActionInputs, values: [:]), [:])
    }

    func testAPickedIconSubmitsItsRegistryName() {
        let values = ActionInputValues.wireValues(
            createActionInputs,
            values: ["icon": "rocket", "repo": "repo-uuid"]
        )
        XCTAssertEqual(values["icon"], "rocket")
        XCTAssertEqual(values, ["icon": "rocket", "repo": "repo-uuid"])
        // Whatever the grid emits is a storable board icon.
        XCTAssertTrue(DomainContract.boardIconValues.contains("rocket"))
    }

    // Clearing the picker writes "" (non-nil, so it is never re-seeded) — the
    // wire map must drop it rather than send an empty glyph name.
    func testAClearedIconIsDroppedFromTheWireValues() {
        let values = ActionInputValues.wireValues(
            createActionInputs,
            values: ["icon": "", "repo": "repo-uuid"]
        )
        XCTAssertNil(values["icon"])
        XCTAssertEqual(values, ["repo": "repo-uuid"])
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

    // A required pick with only whitespace is still unset.
    func testWhitespaceDoesNotCountAsAPick() {
        let pr = ActionInputDto(key: "pr", label: "Pull request", type: "pr", required: true)
        XCTAssertFalse(ActionInputValues.requiredFilled([pr], values: ["pr": "   "]))
    }

    // Keys the action does not declare never reach the wire — the composer's
    // seed may carry an `icon` for an action without an icon input.
    func testUndeclaredKeysAreDropped() {
        let fix = ActionDto.builtinFixConflictsAction(teamId: "t-1").inputs ?? []
        XCTAssertEqual(
            ActionInputValues.wireValues(fix, values: ["pr": "issue-1", "icon": "rocket"]),
            ["pr": "issue-1"]
        )
    }
}
