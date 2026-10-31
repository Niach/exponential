import Foundation
import XCTest
@testable import ExpCore

// S-3 stories, locked ×4 against
// `packages/domain-contract/fixtures/stories.json`: every case, the WHOLE
// expected array. Read through `#filePath`: the unit-test bundle carries no
// repo resources.
final class StoriesTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/stories.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    private func num(_ raw: Any?) -> Double? { (raw as? NSNumber)?.doubleValue }

    private func json(_ ring: Stories.StoryRing) -> [String: Any] {
        func orNull(_ value: Any?) -> Any { value ?? NSNull() }
        return [
            "key": ring.key,
            "agentId": orNull(ring.agentId),
            "rootId": orNull(ring.rootId),
            "title": ring.title,
            "assistant": ring.assistant,
            "waiting": ring.waiting,
            "unseen": ring.unseen,
            "stories": ring.stories.map { story -> [String: Any] in
                [
                    "id": story.id,
                    "sessionId": story.sessionId,
                    "rootId": story.rootId,
                    "sessionName": story.sessionName,
                    "kind": story.kind.rawValue,
                    "waiting": story.waiting,
                    "seen": story.seen,
                    "createdAt": story.createdAt,
                    "meta": story.meta,
                    "title": orNull(story.title),
                    "description": orNull(story.description),
                    "pictures": story.pictures.map { picture -> [String: Any] in
                        [
                            "label": picture.label,
                            "attachmentId": picture.attachmentId,
                            "width": orNull(picture.width),
                            "height": orNull(picture.height),
                            "caption": orNull(picture.caption),
                        ]
                    },
                    "surface": story.surface,
                    "counts": orNull(story.counts.map { counts -> [String: Any] in
                        ["sections": counts.sections, "pictures": counts.pictures, "questions": counts.questions]
                    }),
                    "footer": story.footer.rawValue,
                    "autoAdvanceMs": orNull(story.autoAdvanceMs),
                ]
            },
        ]
    }

    func testConstantsMatchTheFixture() throws {
        let f = try fixture()
        let constants = try XCTUnwrap(f["constants"] as? [String: NSNumber])
        XCTAssertEqual(constants.keys.sorted(), ["autoAdvanceMs", "windowHours"])
        XCTAssertEqual(constants["windowHours"]?.doubleValue, Stories.Constants.windowHours)
        XCTAssertEqual(constants["autoAdvanceMs"]?.doubleValue, Stories.Constants.autoAdvanceMs)
        XCTAssertEqual(f["copy"] as? [String: String], [
            "plan": Stories.Copy.plan, "question": Stories.Copy.question, "result": Stories.Copy.result,
            "now": Stories.Copy.now, "approve": Stories.Copy.approve, "change": Stories.Copy.change,
            "reply": Stories.Copy.reply, "close": Stories.Copy.close, "next": Stories.Copy.next,
            "previous": Stories.Copy.previous,
        ])
        XCTAssertEqual(f["ring"] as? [String: Double], ["width": Stories.Ring.width, "gap": Stories.Ring.gap])
        XCTAssertEqual(
            (f["progress"] as? [String: Any])?.filter { $0.key != "$comment" } as? [String: Double], [
                "height": Stories.Progress.height, "gap": Stories.Progress.gap,
                "storyGap": Stories.Progress.storyGap,
            ]
        )
        let viewer = try XCTUnwrap(f["viewer"] as? [String: Any])
        XCTAssertEqual(viewer.keys.sorted(), ["height", "inset", "minWidth", "radius", "scrimFade", "scrimHeight", "width"])
        XCTAssertEqual(num(viewer["width"]), Stories.Viewer.width)
        XCTAssertEqual(num(viewer["height"]), Stories.Viewer.height)
        XCTAssertEqual(num(viewer["radius"]), Stories.Viewer.radius)
        XCTAssertEqual(num(viewer["scrimHeight"]), Stories.Viewer.scrimHeight)
        XCTAssertEqual(num(viewer["scrimFade"]), Stories.Viewer.scrimFade)
        XCTAssertEqual(viewer["inset"] as? [String: Double], ["x": Stories.Viewer.insetX, "y": Stories.Viewer.insetY])
        XCTAssertEqual(num(viewer["minWidth"]), Stories.Viewer.minWidth)
        XCTAssertEqual(f["row"] as? [String: Double], [
            "itemWidth": Stories.Row.itemWidth, "gap": Stories.Row.gap,
            "labelGap": Stories.Row.labelGap, "padBottom": Stories.Row.padBottom,
        ])
        let badge = try XCTUnwrap(f["badge"] as? [String: Any])
        for kind in [Stories.Kind.plan, .question, .result] {
            XCTAssertEqual((badge["icons"] as? [String: String])?[kind.rawValue], Stories.Badge.icon(kind))
            XCTAssertEqual((badge["tints"] as? [String: String])?[kind.rawValue], Stories.Badge.tint(kind))
        }
        XCTAssertEqual(num(badge["fill"]), Stories.Badge.fill)
        XCTAssertEqual(num(badge["stroke"]), Stories.Badge.stroke)
        XCTAssertEqual(num(badge["height"]), Stories.Badge.height)
        XCTAssertEqual(f["testIds"] as? [String: String], [
            "ring": Stories.TestIds.ring, "viewer": Stories.TestIds.viewer, "progress": Stories.TestIds.progress,
            "header": Stories.TestIds.header, "next": Stories.TestIds.next, "previous": Stories.TestIds.previous,
        ])
    }

    private typealias Inputs = (
        now: Double, sessions: [Stories.SessionInput], agents: [Stories.AgentInput],
        messages: [Stories.MessageInput], reads: [Stories.ReadInput]
    )

    /// `stories(input)` over a fixture input object.
    private func fold(_ input: [String: Any], _ name: String) throws -> [Stories.StoryRing] {
        let i = try inputs(input, name)
        return Stories.rings(now: i.now, sessions: i.sessions, agents: i.agents, messages: i.messages, reads: i.reads)
    }

    private func inputs(_ input: [String: Any], _ name: String) throws -> Inputs {
        let sessions = try XCTUnwrap(input["sessions"] as? [[String: Any]], name).map { raw in
            Stories.SessionInput(
                id: raw["id"] as? String ?? "",
                rootId: raw["rootId"] as? String ?? "",
                name: raw["name"] as? String ?? "",
                agentId: raw["agentId"] as? String,
                startedByAgentId: raw["startedByAgentId"] as? String
            )
        }
        let agents = try XCTUnwrap(input["agents"] as? [[String: Any]], name).map { raw in
            Stories.AgentInput(
                id: raw["id"] as? String ?? "",
                name: raw["name"] as? String ?? "",
                assistant: raw["assistant"] as? Bool ?? false
            )
        }
        let messages = try XCTUnwrap(input["messages"] as? [[String: Any]], name).map { raw in
            Stories.MessageInput(
                id: raw["id"] as? String ?? "",
                sessionId: raw["sessionId"] as? String ?? "",
                authorSessionId: raw["authorSessionId"] as? String,
                createdAt: num(raw["createdAt"]) ?? 0,
                answeredAt: num(raw["answeredAt"]),
                card: (raw["card"] as? [String: Any]).map(TreeMessageCard.init(object:))
            )
        }
        let reads = try XCTUnwrap(input["reads"] as? [[String: Any]], name).map { raw in
            Stories.ReadInput(
                sessionId: raw["sessionId"] as? String ?? "",
                readAt: num(raw["readAt"]),
                storySeenAt: num(raw["storySeenAt"])
            )
        }
        return (try XCTUnwrap(num(input["now"]), name), sessions, agents, messages, reads)
    }

    /// stories.json `tab`: copy, test ids and the phone tile geometry.
    func testTabMatchesTheFixture() throws {
        let tab = try XCTUnwrap(try fixture()["tab"] as? [String: Any])
        XCTAssertEqual(tab["copy"] as? [String: String], [
            "label": Stories.Tab.Copy.label, "empty": Stories.Tab.Copy.empty,
            "approved": Stories.Tab.Copy.approved, "changed": Stories.Tab.Copy.changed,
            "answered": Stories.Tab.Copy.answered, "superseded": Stories.Tab.Copy.superseded,
            "changes": Stories.Tab.Copy.changes, "stack": Stories.Tab.Copy.stack,
        ])
        XCTAssertEqual(tab["testIds"] as? [String: String], [
            "tab": Stories.Tab.TestIds.tab, "list": Stories.Tab.TestIds.list, "tile": Stories.Tab.TestIds.tile,
            "changes": Stories.Tab.TestIds.changes, "stack": Stories.Tab.TestIds.stack,
        ])
        let tile = try XCTUnwrap(tab["tile"] as? [String: Any])
        XCTAssertEqual((tile["thumbHeight"] as? [String: Double])?["phone"], Stories.Tab.Tile.thumbHeight)
        XCTAssertEqual((tile["thumbMaxWidth"] as? [String: Double])?["phone"], Stories.Tab.Tile.thumbMaxWidth)
        XCTAssertEqual(num(tile["gap"]), Stories.Tab.Tile.gap)
        XCTAssertEqual(num(tile["textGap"]), Stories.Tab.Tile.textGap)
        XCTAssertEqual(num(tile["radius"]), Stories.Tab.Tile.radius)
        XCTAssertEqual(num(tile["listGap"]), Stories.Tab.Tile.listGap)
        XCTAssertEqual(num(tile["leadGap"]), Stories.Tab.Tile.leadGap)
        XCTAssertEqual(
            num((tab["specimen"] as? [String: Any])?["case"]).map(Int.init), Stories.Tab.specimenCase
        )
        XCTAssertEqual(Stories.Tab.Tile.thumbWidth(width: 1000, height: 1000), 240)
        XCTAssertEqual(Stories.Tab.Tile.thumbWidth(width: 1179, height: 2556), 111)
        XCTAssertEqual(Stories.Tab.Tile.thumbWidth(width: 4000, height: 1000), 320)
        XCTAssertEqual(Stories.Tab.Tile.thumbWidth(width: nil, height: nil), 320)
    }

    /// `storyTab(input + rootId)` over every `tabCases` entry, the WHOLE ring.
    func testTabCases() throws {
        let cases = try XCTUnwrap(try fixture()["tabCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for entry in cases {
            let name = entry["name"] as? String ?? "?"
            let input = try XCTUnwrap(entry["input"] as? [String: Any], name)
            let i = try inputs(input, name)
            let ring = Stories.storyTab(
                now: i.now, sessions: i.sessions, agents: i.agents, messages: i.messages, reads: i.reads,
                rootId: try XCTUnwrap(input["rootId"] as? String, name)
            )
            if entry["expected"] is NSNull {
                XCTAssertNil(ring, name)
                continue
            }
            let expected = try XCTUnwrap(entry["expected"] as? [String: Any], name)
            let actual = json(try XCTUnwrap(ring, name))
            XCTAssertTrue(
                NSDictionary(dictionary: actual).isEqual(to: expected),
                "\(name)\nactual: \(actual)\nexpected: \(expected)"
            )
        }
    }

    func testCases() throws {
        let cases = try XCTUnwrap(try fixture()["cases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for entry in cases {
            let name = entry["name"] as? String ?? "?"
            let input = try XCTUnwrap(entry["input"] as? [String: Any], name)
            let actual = try fold(input, name).map(json)
            let expected = try XCTUnwrap(entry["expected"] as? [Any], name)
            XCTAssertTrue(
                NSArray(array: actual).isEqual(to: expected),
                "\(name)\nactual: \(actual)\nexpected: \(expected)"
            )
        }
    }

    private func story(_ raw: [String: Any]) -> Stories.Story {
        Stories.Story(
            id: raw["id"] as? String ?? "",
            sessionId: raw["sessionId"] as? String ?? "",
            rootId: raw["rootId"] as? String ?? "",
            sessionName: raw["sessionName"] as? String ?? "",
            kind: Stories.Kind(rawValue: raw["kind"] as? String ?? "") ?? .result,
            waiting: raw["waiting"] as? Bool ?? false,
            seen: raw["seen"] as? Bool ?? false,
            createdAt: num(raw["createdAt"]) ?? 0,
            meta: raw["meta"] as? String ?? "",
            title: raw["title"] as? String,
            description: raw["description"] as? String,
            pictures: (raw["pictures"] as? [[String: Any]] ?? []).map { picture in
                Stories.Picture(
                    label: picture["label"] as? String ?? "",
                    attachmentId: picture["attachmentId"] as? String ?? "",
                    width: num(picture["width"]),
                    height: num(picture["height"]),
                    caption: picture["caption"] as? String
                )
            },
            surface: raw["surface"] as? Bool ?? false,
            counts: (raw["counts"] as? [String: Any]).map { counts in
                PlanPage.Counts(
                    sections: (counts["sections"] as? Int) ?? 0,
                    pictures: (counts["pictures"] as? Int) ?? 0,
                    questions: (counts["questions"] as? Int) ?? 0
                )
            },
            footer: Stories.Footer(rawValue: raw["footer"] as? String ?? "") ?? .none,
            autoAdvanceMs: num(raw["autoAdvanceMs"])
        )
    }

    /// #EXP-1493 `storyFrames(ring)` over every `frameCases` entry, the WHOLE
    /// array: ONE frame per story.
    func testFrameCases() throws {
        let cases = try XCTUnwrap(try fixture()["frameCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        func orNull(_ value: Any?) -> Any { value ?? NSNull() }
        for entry in cases {
            let name = entry["name"] as? String ?? "?"
            let input = try XCTUnwrap(entry["input"] as? [String: Any], name)
            let stories = try XCTUnwrap(input["stories"] as? [[String: Any]], name).map(story)
            let actual = Stories.frames(stories).map { frame -> [String: Any] in
                [
                    "story": frame.story,
                    "storyId": frame.storyId,
                    "kind": frame.kind.rawValue,
                    "pictures": frame.pictures,
                    "title": orNull(frame.title),
                    "description": orNull(frame.description),
                    "footer": frame.footer.rawValue,
                    "autoAdvanceMs": orNull(frame.autoAdvanceMs),
                ]
            }
            let expected = try XCTUnwrap(entry["expected"] as? [Any], name)
            XCTAssertTrue(
                NSArray(array: actual).isEqual(to: expected),
                "\(name)\nactual: \(actual)\nexpected: \(expected)"
            )
            // A tile (or a thumbnail) opens at its story's frame.
            for (at, frame) in Stories.frames(stories).enumerated() {
                XCTAssertEqual(Stories.frameIndex(Stories.frames(stories), storyId: frame.storyId), at, name)
            }
        }
        XCTAssertEqual(Stories.frameIndex([], storyId: "x"), -1)
    }

    /// #EXP-1461 THE STORY ROW (TS `storyRow`): every `rowCases` entry,
    /// `bubbles` and the `rest` keys.
    func testRowCases() throws {
        let cases = try XCTUnwrap(try fixture()["rowCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for entry in cases {
            let name = entry["name"] as? String ?? "?"
            let input = try XCTUnwrap(entry["input"] as? [String: Any], name)
            let rings = try XCTUnwrap(input["rings"] as? [[String: Any]], name).map { raw in
                Stories.StoryRing(
                    key: raw["key"] as? String ?? "", agentId: raw["agentId"] as? String, rootId: nil,
                    title: "", assistant: false, waiting: 0, unseen: false, stories: []
                )
            }
            let row = Stories.storyRow(try XCTUnwrap(input["bubbleAgentIds"] as? [String], name), rings)
            let actual: [String: Any] = ["bubbles": row.bubbles, "rest": row.rest.map(\.key)]
            let expected = try XCTUnwrap(entry["expected"] as? [String: Any], name)
            XCTAssertTrue(NSDictionary(dictionary: actual).isEqual(to: expected), "\(name)\nactual: \(actual)")
            for id in row.bubbles { XCTAssertEqual(row.byAgent[id]?.agentId, id, name) }
        }
    }

    /// #EXP-1463 (TS `storyViewerSize`): every `sizeCases` entry.
    func testSizeCases() throws {
        let cases = try XCTUnwrap(try fixture()["sizeCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for entry in cases {
            let name = entry["name"] as? String ?? "?"
            let input = try XCTUnwrap(entry["input"] as? [String: Any], name)
            let viewport = try XCTUnwrap(input["viewport"] as? [String: Double], name)
            let picture = (input["picture"] as? [String: Any]).map { (width: num($0["width"]), height: num($0["height"])) }
            let size = Stories.viewerSize(
                picture: picture, viewport: (width: viewport["width"] ?? 0, height: viewport["height"] ?? 0)
            )
            XCTAssertEqual(["width": size.width, "height": size.height], entry["expected"] as? [String: Double], name)
        }
    }
}
