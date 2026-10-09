import SwiftUI
import XCTest
@testable import ExpUI

// The UI cleanup batch: the iOS consolidation onto the shared primitives —
// THE repository + branch pickers, the untyped picker row and the switch row.
final class RepositoryPickerTests: XCTestCase {
    /// The pickers that retired the hand-rolled sheets render THROUGH the
    /// primitive, exactly like the ten typed ones (`PickerContractTests`).
    func testRetiredSheetsRenderThroughThePrimitive() {
        let bodies: [(String, Any.Type)] = [
            ("RepositoryPicker", RepositoryPicker<EmptyView>.Body.self),
            ("BranchPicker", BranchPicker<EmptyView>.Body.self),
            ("GlassPickerRow", GlassPickerRow<String>.Body.self),
        ]
        for (name, body) in bodies {
            XCTAssertTrue(
                String(describing: body).hasPrefix("GlassPicker<"),
                "\(name) renders \(body) instead of the primitive"
            )
        }
    }

    func testItemsLeadWithTheNoneRowAndMarkReposWithTheGithubGlyph() {
        let rows = [
            RepositoryPickerRow(id: "r1", fullName: "acme/web", isPrivate: true),
            RepositoryPickerRow(id: "r2", fullName: "acme/api", tag: "matches board", emphasis: true),
        ]
        let items = RepositoryPickerRules.items(rows, noneLabel: "No repository")
        XCTAssertEqual(items.map(\.value), [RepositoryPickerRules.noneValue, "r1", "r2"])
        XCTAssertEqual(items.map(\.label), ["No repository", "acme/web", "acme/api"])
        XCTAssertEqual(items[0].mark, .plain)
        XCTAssertEqual(items[1].mark, .glyph(AppIcons.uiGithub, nil))
        XCTAssertEqual(items[2].searchKeywords, ["acme/api"])
        XCTAssertEqual(RepositoryPickerRules.items(rows).count, 2)
    }

    /// Web's rule: the filter field turns on past seven repositories.
    func testSearchTurnsOnPastSeven() {
        XCTAssertFalse(RepositoryPickerRules.showsSearch(count: 7))
        XCTAssertTrue(RepositoryPickerRules.showsSearch(count: 8))
    }

    func testListFilterIsACaseInsensitiveContainsOnTheFullName() {
        let rows = [
            RepositoryPickerRow(id: "1", fullName: "Acme/Web"),
            RepositoryPickerRow(id: "2", fullName: "acme/api"),
        ]
        XCTAssertEqual(RepositoryPickerRules.filter(rows, query: " web ").map(\.id), ["1"])
        XCTAssertEqual(RepositoryPickerRules.filter(rows, query: "ACME").map(\.id), ["1", "2"])
        XCTAssertEqual(RepositoryPickerRules.filter(rows, query: "").map(\.id), ["1", "2"])
    }

    /// The effective branch is always offered, even when GitHub dropped it;
    /// picking the repo default reports nil (follow the repo again).
    func testBranchRowsKeepTheEffectiveBranchAndTheDefaultUnpins() {
        XCTAssertEqual(
            BranchPickerRules.names(branches: ["master", "dev"], value: "gone"),
            ["gone", "master", "dev"]
        )
        XCTAssertEqual(BranchPickerRules.names(branches: ["master", "dev"], value: "dev"), ["master", "dev"])
        XCTAssertNil(BranchPickerRules.pin("master", repoDefault: "master"))
        XCTAssertEqual(BranchPickerRules.pin("dev", repoDefault: "master"), "dev")
    }

    /// The pill switch is the glass switch at the 28pt row's scale: the same
    /// 2pt thumb inset as the 51×31 one.
    func testPillSwitchKeepsTheGlassSwitchProportions() {
        XCTAssertEqual(GlassToggleRowTokens.pillHeight, 28)
        XCTAssertEqual(GlassToggleRowTokens.pillTrackHeight - GlassToggleRowTokens.pillThumb, 4)
        XCTAssertLessThan(GlassToggleRowTokens.pillTrackHeight, GlassToggleRowTokens.pillHeight)
    }
}
