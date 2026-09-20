import Foundation
import XCTest
import ExpCore
@testable import ExpUI

// EXP-920: the entity chip's glyph seam. `EntityPreview.icon` (ExpCore, the
// contract table) names a CONCEPT per kind; `AppIcons` ships one static
// member per concept rather than a lookup, so `EntityChipIcon.glyphs` is the
// hand list that bridges the two. What is pinned here is that the bridge is
// complete — every contract kind lands on a shipped imageset — and that it
// says the same thing as the generated members it names.
final class EntityChipTests: XCTestCase {
    func testEveryContractKindResolvesToAShippedGlyph() {
        for kind in DomainContract.entityRefKindValues {
            let ref = EntityRef(kind: kind, id: "x")
            let concept = EntityPreview.refIcon(ref)
            XCTAssertNotNil(EntityChipIcon.glyphs[concept], "\(kind) → \(concept) has no glyph entry")
            let glyph = EntityChipIcon.glyph(for: ref)
            XCTAssertNotNil(AppIcons.assetName(glyph), "\(kind) → \(concept) → \(glyph) is not a shipped imageset")
        }
        // A list chip draws its MEMBER kind's glyph.
        XCTAssertEqual(
            EntityChipIcon.glyph(for: EntityRef(kind: "list", id: "session")), AppIcons.codingRunning
        )
        // An unknown concept (a newer contract) falls back to the list glyph.
        XCTAssertEqual(EntityChipIcon.glyph(forConcept: "nav-mystery"), AppIcons.uiChecklist)
    }

    /// The hand list covers exactly the contract's concept table — no kind
    /// without a glyph, no stray concept nobody draws.
    func testTheGlyphTableMirrorsTheContractConcepts() {
        XCTAssertEqual(Set(EntityChipIcon.glyphs.keys), Set(EntityPreview.icon.values))
    }

    /// The generated member each concept names, spelled out so a re-pointed
    /// concept in `icons.json` shows up here as a diff, not a silent swap.
    func testTheGlyphTableNamesTheGeneratedMembers() {
        XCTAssertEqual(EntityChipIcon.glyphs["ui-issue"], AppIcons.uiIssue)
        XCTAssertEqual(EntityChipIcon.glyphs["nav-boards"], AppIcons.navBoards)
        XCTAssertEqual(EntityChipIcon.glyphs["nav-actions"], AppIcons.navActions)
        XCTAssertEqual(EntityChipIcon.glyphs["nav-automations"], AppIcons.navAutomations)
        XCTAssertEqual(EntityChipIcon.glyphs["notification-issue-comment"], AppIcons.notificationIssueComment)
        XCTAssertEqual(EntityChipIcon.glyphs["coding-running"], AppIcons.codingRunning)
        XCTAssertEqual(EntityChipIcon.glyphs["settings-labels"], AppIcons.settingsLabels)
        XCTAssertEqual(EntityChipIcon.glyphs["settings-statuses"], AppIcons.settingsStatuses)
        XCTAssertEqual(EntityChipIcon.glyphs["nav-workflows"], AppIcons.navWorkflows)
        XCTAssertEqual(EntityChipIcon.glyphs["ui-device"], AppIcons.uiDevice)
        XCTAssertEqual(EntityChipIcon.glyphs["ui-avatar-placeholder"], AppIcons.uiAvatarPlaceholder)
        XCTAssertEqual(EntityChipIcon.glyphs["ui-repository"], AppIcons.uiRepository)
        XCTAssertEqual(EntityChipIcon.glyphs["ui-team"], AppIcons.uiTeam)
        XCTAssertEqual(EntityChipIcon.glyphs["ui-invite"], AppIcons.uiInvite)
        XCTAssertEqual(EntityChipIcon.glyphs["nav-notifications"], AppIcons.navNotifications)
        XCTAssertEqual(EntityChipIcon.glyphs["nav-support"], AppIcons.navSupport)
        XCTAssertEqual(EntityChipIcon.glyphs["ui-attach"], AppIcons.uiAttach)
        XCTAssertEqual(EntityChipIcon.glyphs["ui-checklist"], AppIcons.uiChecklist)
    }
}
