import Foundation
import XCTest
@testable import ExpCore

// EXP-920: the entity-preview chip rule, locked ×4 (web `entity-preview.test.ts`,
// desktop `domain::entity_preview`, Android `EntityPreviewTest`) against the
// ONE contract fixture — same cases, same test names.
final class EntityPreviewTests: XCTestCase {
    private struct ChipCase {
        let name: String
        let ref: EntityRef
        let label: String
        let detail: String?
        let icon: String
    }

    private struct GroupCase {
        let name: String
        let refs: [EntityRef]
        /// Each group as indexes into `refs`: the chip's ref, then its members.
        let groups: [(ref: Int, members: [Int])]
    }

    /// The repo root, walked up from this file — the unit-test bundle carries
    /// no repo resources.
    private var repoRoot: URL {
        URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
    }

    private func fixture() throws -> [String: Any] {
        let url = repoRoot.appendingPathComponent("packages/domain-contract/fixtures/entity-chip.json")
        let data = try Data(contentsOf: url)
        return try XCTUnwrap(try JSONSerialization.jsonObject(with: data) as? [String: Any])
    }

    /// The fixture's refs are the WIRE shape, so they go through the parser —
    /// except the deliberately unknown kinds, which the parser (rightly)
    /// drops and the rule still has to label.
    private func ref(_ raw: [String: Any]) throws -> EntityRef {
        if let parsed = EntityRef.parse(raw) { return parsed }
        return EntityRef(
            kind: try XCTUnwrap(raw["kind"] as? String),
            id: try XCTUnwrap(raw["id"] as? String),
            identifier: raw["identifier"] as? String,
            title: raw["title"] as? String,
            count: (raw["count"] as? NSNumber)?.intValue
        )
    }

    private func chipCases() throws -> [ChipCase] {
        let raw = try XCTUnwrap(try fixture()["chips"] as? [[String: Any]])
        return try raw.map { object in
            ChipCase(
                name: try XCTUnwrap(object["name"] as? String),
                ref: try ref(try XCTUnwrap(object["ref"] as? [String: Any])),
                label: try XCTUnwrap(object["label"] as? String),
                detail: object["detail"] as? String,
                icon: try XCTUnwrap(object["icon"] as? String)
            )
        }
    }

    private func groupCases() throws -> [GroupCase] {
        let raw = try XCTUnwrap(try fixture()["groups"] as? [[String: Any]])
        return try raw.map { object in
            let refs = try XCTUnwrap(object["refs"] as? [[String: Any]]).map { try ref($0) }
            let groups = try XCTUnwrap(object["groups"] as? [[String: Any]]).map { group in
                (
                    ref: try XCTUnwrap(group["ref"] as? Int),
                    members: try XCTUnwrap(group["members"] as? [Int])
                )
            }
            return GroupCase(
                name: try XCTUnwrap(object["name"] as? String),
                refs: refs,
                groups: groups
            )
        }
    }

    func testEveryChipCaseRendersByteExact() throws {
        let cases = try chipCases()
        XCTAssertGreaterThanOrEqual(cases.count, 30)
        for fixture in cases {
            XCTAssertEqual(EntityPreview.chipLabel(fixture.ref), fixture.label, fixture.name)
            XCTAssertEqual(EntityPreview.chipDetail(fixture.ref), fixture.detail, fixture.name)
            XCTAssertEqual(EntityPreview.refIcon(fixture.ref), fixture.icon, fixture.name)
        }
    }

    func testEveryGroupCaseGroupsTheSame() throws {
        let cases = try groupCases()
        XCTAssertGreaterThanOrEqual(cases.count, 7)
        for fixture in cases {
            let groups = EntityPreview.groupRefs(fixture.refs)
            let expected = fixture.groups.map { group in
                EntityPreview.Group(
                    ref: fixture.refs[group.ref],
                    members: group.members.map { fixture.refs[$0] }
                )
            }
            XCTAssertEqual(groups, expected, fixture.name)
        }
    }

    /// Every contract kind draws a concept, and every concept is one the icon
    /// registry ships (`packages/icons/icons.json` `semantic`) — the ExpUI
    /// twin (`EntityChipTests`) locks the `AppIcons` member each resolves to.
    func testEveryKindHasARegistryIconConcept() throws {
        let url = repoRoot.appendingPathComponent("packages/icons/icons.json")
        let data = try Data(contentsOf: url)
        let json = try XCTUnwrap(try JSONSerialization.jsonObject(with: data) as? [String: Any])
        let semantic = try XCTUnwrap(json["semantic"] as? [String: Any])
        XCTAssertEqual(
            Set(EntityPreview.icon.keys), Set(DomainContract.entityRefKindValues),
            "one icon concept per contract kind, nothing else"
        )
        for kind in DomainContract.entityRefKindValues {
            let concept = try XCTUnwrap(EntityPreview.icon[kind], kind)
            XCTAssertNotNil(semantic[concept], "\(kind) → \(concept) is not a registry concept")
        }
        XCTAssertEqual(EntityPreview.chipLabelMax, 48)
    }

    // MARK: - The wire parser (web `parseEntityRef`)

    func testParseKeepsAKnownKindAndDropsTheRest() {
        let parsed = EntityRef.parse([
            "kind": "issue", "id": "  i-1 ", "identifier": " EXP-42 ", "title": "Fix", "count": 2.6,
        ])
        XCTAssertEqual(parsed, EntityRef(kind: "issue", id: "i-1", identifier: "EXP-42", title: "Fix", count: 3))
        XCTAssertNil(EntityRef.parse(nil))
        XCTAssertNil(EntityRef.parse("issue"))
        XCTAssertNil(EntityRef.parse(["kind": "thing", "id": "x-1"]), "an unknown kind is dropped")
        XCTAssertNil(EntityRef.parse(["kind": "issue", "id": "   "]), "a blank id is no ref")
        XCTAssertNil(EntityRef.parse(["kind": "issue"]))
        // Blank strings count as absent, a negative count is dropped.
        let sparse = EntityRef.parse(["kind": "board", "id": "b-1", "title": "  ", "count": -1])
        XCTAssertEqual(sparse, EntityRef(kind: "board", id: "b-1"))
        // Every string is cut to the contract's text cap.
        let long = String(repeating: "x", count: 400)
        let cut = EntityRef.parse(["kind": "comment", "id": long, "title": long])
        XCTAssertEqual(cut?.id.count, DomainContract.expToolPreviewTextMax)
        XCTAssertEqual(cut?.title?.count, DomainContract.expToolPreviewTextMax)
    }

    func testTheNounsAndTheClamp() {
        XCTAssertEqual(EntityPreview.kindNoun("session", count: 1), "run")
        XCTAssertEqual(EntityPreview.kindNoun("session", count: 0), "runs")
        XCTAssertEqual(EntityPreview.kindNoun("thing"), "item")
        XCTAssertEqual(EntityPreview.clampChipLabel("  short  "), "short")
        let exact = String(repeating: "a", count: 48)
        XCTAssertEqual(EntityPreview.clampChipLabel(exact), exact)
        let over = String(repeating: "a", count: 46) + "  b"
        XCTAssertEqual(EntityPreview.clampChipLabel(over), String(repeating: "a", count: 46) + "…")
    }
}
