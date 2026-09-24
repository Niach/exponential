import SwiftUI
import XCTest
import ExpCore
import ExpUI

// EXP-1029 contract, EXP-1021 implementation — the shared picker API on iOS.
// Web, the IDE and Android carry the same case names; what differs is only
// how a platform can see its own view tree. SwiftUI has no DOM to query, so
// the presentation rules are pinned where they actually live: as the pure
// functions the sheet body calls (`PickerSearch`, `PickerSelection`), as the
// tokens it paints with (`GlassPickerTokens`), and as the STRUCTURE of the
// types themselves — a typed picker's `Body` IS the primitive, and there is
// exactly one `PickerSelectionStyle` case, so a circle cannot come back.
// The typed pickers are views, so a case that reaches for one stays on the
// main actor (EXP-1020 hit this first; the `items` statics are `nonisolated`,
// but the annotation costs nothing and keeps the target building under both
// toolchains).
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

    /// An account row is keyed by the option's `key`, never the bare profile
    /// id (`system` repeats across agents), searches on the email AND the
    /// agent, and falls back to the health badge as its second line — the
    /// EXP-992 bars replace that line when the row draws its own body.
    func testAnAccountRowIsKeyedByItsAgentAndProfile() {
        let rows = AccountPicker<EmptyView>.items([
            AccountOption(
                id: "system",
                agent: "claude",
                email: "ada@exp.dev",
                isDeviceDefault: true,
                health: .needsRelogin
            ),
            AccountOption(id: "system", agent: "codex", email: "ada@exp.dev", isDeviceDefault: false),
        ])
        XCTAssertEqual(rows.map(\.value), ["claude:system", "codex:system"])
        XCTAssertEqual(rows.map(\.label), ["ada@exp.dev", "ada@exp.dev"])
        XCTAssertEqual(rows[0].description, AgentAccountHealth.needsRelogin.badgeLabel)
        XCTAssertNil(rows[1].description)
        XCTAssertEqual(rows[1].searchKeywords, ["ada@exp.dev", "codex"])
    }

    /// All ten, including the two EXP-1021 re-homed (the account picker and
    /// the icon picker's grid). A typed picker's `body` is `some View`, so its
    /// `Body` is the concrete type it returns: if any of them ever renders its
    /// own sheet again, its `Body` stops being `GlassPicker` and this fails.
    func testEveryTypedPickerRendersThroughThePrimitive() {
        let typed: [(String, Any.Type)] = [
            ("BoardPicker", BoardPicker<EmptyView>.Body.self),
            ("IssuePicker", IssuePicker<EmptyView>.Body.self),
            ("ActionPicker", ActionPicker<EmptyView>.Body.self),
            ("AccountPicker", AccountPicker<EmptyView>.Body.self),
            ("DevicePicker", DevicePicker<EmptyView>.Body.self),
            ("AssigneePicker", AssigneePicker<EmptyView>.Body.self),
            ("IconPicker", IconPicker.Body.self),
            ("StatusPicker", StatusPicker<EmptyView>.Body.self),
            ("PriorityPicker", PriorityPicker<EmptyView>.Body.self),
            ("LabelPicker", LabelPicker<EmptyView>.Body.self),
        ]
        XCTAssertEqual(typed.count, 10)
        for (name, body) in typed {
            XCTAssertTrue(
                String(describing: body).hasPrefix("GlassPicker<"),
                "\(name) renders \(body) instead of the primitive"
            )
        }
    }

    /// The whole point of EXP-1021: the sheet's rows sit on the sheet itself.
    /// A resting row has NO fill and NO hairline — there is nothing between
    /// `GlassSheetChrome` and the list, and the row list's insets are the same
    /// ones every other glass sheet uses.
    func testAPhoneSheetOfPlainRowsNoCardsInsideTheSheet() {
        XCTAssertEqual(GlassPickerTokens.restingFill, .clear)
        XCTAssertEqual(GlassPickerTokens.restingStroke, .clear)
        XCTAssertNotEqual(GlassPickerTokens.restingFill, GlassTokens.fillCard)
        XCTAssertNotEqual(GlassPickerTokens.restingStroke, GlassTokens.strokeCard)
        // The ONE sheet shell, and the ONE row list geometry (EXP-687).
        XCTAssertEqual(GlassPickerTokens.rowMinHeight, 44)
        XCTAssertEqual(GlassPickerTokens.listHPadding, 6)
        XCTAssertEqual(GlassPickerTokens.rowRadius, GlassTokens.rowRadius)
    }

    /// A picked row reads as the row's own highlight — the one bright glass
    /// fill and its paired stroke. `PickerSelectionStyle` has exactly ONE
    /// case, so there is no circle or checkmark to fall back to.
    func testMultiModeMarksPickedRowsByTheHighlightColourNeverACircle() {
        XCTAssertEqual(PickerSelectionStyle.allCases, [.highlight])
        XCTAssertEqual(GlassPickerTokens.selectionStyle, .highlight)
        XCTAssertEqual(GlassPickerTokens.pickedFill, GlassTokens.fillActive)
        XCTAssertEqual(GlassPickerTokens.pickedStroke, GlassTokens.strokeActive)
        XCTAssertNotEqual(GlassPickerTokens.pickedFill, GlassPickerTokens.restingFill)
    }

    func testSingleModeClosesOnAPickMultiModeStaysOpen() {
        XCTAssertTrue(PickerSelection.closesOnPick(.single))
        XCTAssertFalse(PickerSelection.closesOnPick(.multi))
        // Single REPLACES: the sheet has no third state, so picking the
        // current row re-reports it rather than clearing it.
        XCTAssertEqual(PickerSelection.picking("b", in: ["a"], mode: .single), ["b"])
        XCTAssertEqual(PickerSelection.picking("a", in: ["a"], mode: .single), ["a"])
        // Multi TOGGLES and reports the whole new set.
        XCTAssertEqual(PickerSelection.picking("b", in: ["a"], mode: .multi), ["a", "b"])
        XCTAssertEqual(PickerSelection.picking("a", in: ["a"], mode: .multi), [])
    }

    func testSearchFiltersRowsByLabelAndKeywords() {
        let items = [
            PickerItem(value: "a", label: "Alpha", keywords: ["APP-1", "Alpha"]),
            PickerItem(value: "b", label: "Beta", keywords: ["APP-2", "Beta"]),
            PickerItem(value: "c", label: "Gamma"),
        ]
        XCTAssertEqual(PickerSearch.filter(items, query: "").map(\.value), ["a", "b", "c"])
        XCTAssertEqual(PickerSearch.filter(items, query: "   ").map(\.value), ["a", "b", "c"])
        XCTAssertEqual(PickerSearch.filter(items, query: "Bet").map(\.value), ["b"])
        // A keyword the label does not contain still matches …
        XCTAssertEqual(PickerSearch.filter(items, query: "APP-1").map(\.value), ["a"])
        // … and a row with no keywords falls back to its label, case-blind.
        XCTAssertEqual(PickerSearch.filter(items, query: "gam").map(\.value), ["c"])
        XCTAssertEqual(PickerSearch.filter(items, query: "zzz").map(\.value), [])
    }

    /// A disabled row is INFORMATION — an offline device says why it cannot
    /// run — so it renders; it simply never picks (`.disabled` on the row's
    /// button, which is why this asserts the flag reaches the item).
    func testADisabledRowRendersButNeverPicks() {
        let offline = DevicePicker<EmptyView>.items([
            DevicePickerDevice(id: "d", name: "Studio", description: "Offline", disabled: true)
        ])
        XCTAssertEqual(offline.map(\.disabled), [true])
        XCTAssertEqual(offline.map(\.description), ["Offline"])
        // A disabled row still matches the filter: hiding it would lose the
        // reason it is there.
        XCTAssertEqual(PickerSearch.filter(offline, query: "stud").count, 1)
    }

    /// A colour WITH a glyph tints the glyph; a colour WITHOUT one draws the
    /// row's dot. That is how a board row reads as a tinted glyph and a label
    /// row as a coloured dot without any call site choosing a shape.
    func testARowDrawsItsIconInItsColourElseAColouredDot() {
        let board = BoardPicker<EmptyView>.items([
            BoardPickerBoard(id: "b", name: "Web", icon: "flag", colorHex: "#3B82F6")
        ])[0]
        XCTAssertEqual(board.mark, .glyph("flag", Color(hex: "#3B82F6")))

        let label = LabelPicker<EmptyView>.items([
            LabelPickerLabel(id: "l", name: "bug", colorHex: "#EF4444")
        ])[0]
        XCTAssertEqual(label.mark, .dot(Color(hex: "#EF4444")!))

        XCTAssertEqual(PickerItem(value: "x", label: "Plain").mark, .plain)
    }

    /// A bulk edit's third state: a label on ALL of the selected issues reads
    /// fully picked, one on SOME of them partial, and both are weights of the
    /// SAME highlight — the wash without its stroke, never a check-and-minus
    /// column. Web spells it `boolean | "indeterminate"`, Android
    /// `PickerChecked`; the paint is identical.
    func testAPartialPickIsTheSameHighlightWithoutItsStroke() {
        XCTAssertEqual(PickerChecked.allCases, [.none, .some, .all])
        // Explicit `checked` WINS over membership …
        let partial = PickerItem(value: "l", label: "bug", checked: PickerChecked.some)
        XCTAssertEqual(partial.checkedState(in: []), .some)
        XCTAssertEqual(partial.checkedState(in: ["l"]), .some)
        // … and without one the state is derived from it.
        let plain = PickerItem(value: "l", label: "bug")
        XCTAssertEqual(plain.checkedState(in: ["l"]), .all)
        XCTAssertEqual(plain.checkedState(in: []), .none)
        // The partial paint is the picked wash MINUS the stroke.
        XCTAssertEqual(GlassPickerTokens.partialFill, GlassPickerTokens.pickedFill)
        XCTAssertEqual(GlassPickerTokens.partialStroke, .clear)
        XCTAssertNotEqual(GlassPickerTokens.partialStroke, GlassPickerTokens.pickedStroke)
        // A typed picker carries it through: the board bulk label edit.
        XCTAssertEqual(
            LabelPicker<EmptyView>.items([
                LabelPickerLabel(id: "l", name: "bug", checked: PickerChecked.some)
            ]).map(\.checked),
            [PickerChecked.some]
        )
    }

    /// An issue row keeps its STATUS glyph: the relations linker's rows have
    /// always led with it, and that list is the look EXP-1021 is measured
    /// against. The LABEL stays the one-line `IDENT Title` — that half is the
    /// ×4 contract and the fixture; only the leading mark is per-issue.
    func testAnIssueRowLeadsWithItsStatusGlyph() {
        let row = IssuePicker<EmptyView>.items([
            IssuePickerIssue(
                id: "i", identifier: "APP-1", title: "Fix", icon: "circle-dashed",
                colorHex: "#22C55E"
            )
        ])[0]
        XCTAssertEqual(row.label, "APP-1 Fix")
        XCTAssertEqual(row.mark, .glyph("circle-dashed", Color(hex: "#22C55E")))
        // A token colour (a builtin status) wins over the hex, as everywhere.
        let builtin = IssuePicker<EmptyView>.items([
            IssuePickerIssue(
                id: "i", identifier: "APP-1", title: "Fix", icon: "circle-dashed",
                colorHex: "#22C55E", color: .red
            )
        ])[0]
        XCTAssertEqual(builtin.mark, .glyph("circle-dashed", .red))
        // No status handed in = no mark, never a stand-in glyph.
        XCTAssertEqual(
            IssuePicker<EmptyView>.items([
                IssuePickerIssue(id: "i", identifier: "APP-1", title: "Fix")
            ])[0].mark,
            .plain
        )
    }
}
