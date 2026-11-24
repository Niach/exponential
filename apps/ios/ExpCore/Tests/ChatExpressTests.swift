@testable import ExpCore
import Foundation
import XCTest
#if EXPCORE_EUI
import ExponentialUICore
#endif

// #EXP-2067: ChatExpress (TS reference `src/chat-express.ts`) against
// fixtures/chat-express.json. segmentCases + streamCases compile, so they
// replay over the REAL eui SDK (the FFI) when `swift test` links it
// (Package.swift `hasEui`); actionCases, inputCases, plainProseCases,
// previewCases and the constants always run. failureNoteCases are the
// engine's (crates/eagent), not a reader's.
final class ChatExpressTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/chat-express.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    private func cases(_ key: String) throws -> [[String: Any]] {
        let list = try XCTUnwrap(try fixture()[key] as? [[String: Any]], key)
        XCTAssertFalse(list.isEmpty, key)
        return list
    }

    private func assertJSON(_ got: Any, _ expected: Any?, _ message: String, file: StaticString = #filePath, line: UInt = #line) {
        let lhs = try? JSONSerialization.data(withJSONObject: ["v": got], options: [.sortedKeys])
        let rhs = try? JSONSerialization.data(withJSONObject: ["v": expected ?? NSNull()], options: [.sortedKeys])
        XCTAssertEqual(
            lhs.flatMap { String(data: $0, encoding: .utf8) },
            rhs.flatMap { String(data: $0, encoding: .utf8) },
            message, file: file, line: line
        )
    }

    // MARK: - Constants

    func testConstants() throws {
        let f = try fixture()
        let limits = try XCTUnwrap(f["limits"] as? [String: Int])
        XCTAssertEqual(limits["blocksPerRowMax"], ChatExpress.Limits.blocksPerRowMax)
        XCTAssertEqual(limits["blockMaxBytes"], ChatExpress.Limits.blockMaxBytes)
        XCTAssertEqual(limits["blockMaxSurfaces"], ChatExpress.Limits.blockMaxSurfaces)
        XCTAssertEqual(limits["blockMaxComponents"], ChatExpress.Limits.blockMaxComponents)
        XCTAssertEqual(limits["actionContextMaxBytes"], ChatExpress.Limits.actionContextMaxBytes)
        XCTAssertEqual(limits["actionNameMax"], ChatExpress.Limits.actionNameMax)
        XCTAssertEqual(limits["noteLineMax"], ChatExpress.Limits.noteLineMax)
        XCTAssertEqual((f["layout"] as? [String: Any])?["blockGap"] as? Double, ChatExpress.Layout.blockGap)
        let copy = try XCTUnwrap(f["copy"] as? [String: String])
        XCTAssertEqual(copy["blockFailed"], ChatExpress.Copy.blockFailed)
        XCTAssertEqual(copy["blockOnly"], ChatExpress.Copy.blockOnly)
        XCTAssertEqual(copy["actionMarker"], ChatExpress.Copy.actionMarker)
        XCTAssertEqual(copy["you"], ChatExpress.Copy.you)
        XCTAssertEqual(copy["actionTooLarge"], ChatExpress.Copy.actionTooLarge)
        XCTAssertEqual(f["replyEvent"] as? String, ChatExpress.replyEvent)
        XCTAssertEqual(f["gate"] as? String, ChatExpress.gate)
    }

    // MARK: - Gate and held tail (no SDK)

    func testGateFailsAreNil() throws {
        for c in try cases("segmentCases") where c["expected"] is NSNull {
            let text = try XCTUnwrap(c["text"] as? String)
            XCTAssertFalse(ChatExpress.hasExpressBlock(text), c["name"] as? String ?? "")
            XCTAssertNil(ChatExpress.expressRow(text, rowKey: "m:m1", final: true, sdk: NoSdk()))
            XCTAssertEqual(ChatExpress.expressProse(nil, text: text), c["prose"] as? String)
        }
    }

    func testHeldTail() {
        XCTAssertEqual(ChatExpress.withoutHeldTail("Checking the deploy.\n<a2", final: false), "Checking the deploy.")
        XCTAssertEqual(ChatExpress.withoutHeldTail("Checking the deploy.\n<a2", final: true), "Checking the deploy.\n<a2")
        XCTAssertEqual(ChatExpress.withoutHeldTail("<a2ui", final: false), "")
        XCTAssertEqual(ChatExpress.withoutHeldTail("a <b", final: false), "a <b")
        XCTAssertEqual(ChatExpress.withoutHeldTail("x\n  <", final: false), "x")
        // The held tail of a row that holds no block yet: its prose alone.
        XCTAssertEqual(
            ChatExpress.expressRow("Checking the deploy.\n<a2", rowKey: "n:s1:msg_07", final: false, sdk: NoSdk()),
            [.prose("Checking the deploy.")]
        )
    }

    func testRowKeys() {
        XCTAssertEqual(ChatExpress.syncedRowKey("m1"), "m:m1")
        XCTAssertEqual(ChatExpress.feedRowKey(sessionId: "s1", messageId: "msg_07", firstSeq: 4), "n:s1:msg_07")
        XCTAssertEqual(ChatExpress.feedRowKey(sessionId: "s1", messageId: nil, firstSeq: 4), "e:s1:4")
    }

    func testIssueCategory() {
        XCTAssertEqual(ChatExpress.issueCategory("validation error: x").category, "ValidationError")
        XCTAssertEqual(ChatExpress.issueCategory("validation error: x").message, "x")
        XCTAssertEqual(ChatExpress.issueCategory("catalog error: y").category, "CatalogError")
        XCTAssertEqual(ChatExpress.issueCategory("parse error: z").category, "ParseError")
        XCTAssertEqual(ChatExpress.issueCategory("odd").category, "ParseError")
        XCTAssertEqual(ChatExpress.issueCategory("odd").message, "odd")
    }

    // MARK: - Segments and streams (the SDK)

    /// A row as the fixture writes it (no messages, no generation, no SDK words).
    private func locked(_ segments: [ChatExpress.RowSegment]?) -> Any {
        guard let segments else { return NSNull() }
        return segments.map { segment -> [String: Any] in
            switch segment {
            case let .prose(text):
                return ["kind": "prose", "text": text]
            case let .block(state):
                var out: [String: Any] = [
                    "kind": "block", "key": state.key, "raw": state.raw, "closed": state.closed,
                    "status": state.status.rawValue, "surfaces": state.surfaces,
                ]
                if let error = state.error { out["error"] = error }
                return out
            }
        }
    }

    private func sdk() throws -> ExpressSdk {
        #if EXPCORE_EUI
        return ExpressJSONSdk(
            unwrapJson: { expressUnwrapJson(text: $0) },
            stream: { surfaceId in
                let decoder = ExpressDecoder(wrapped: false, surfaceId: surfaceId)
                return (push: { decoder.push(chunk: $0) }, end: { decoder.end() })
            }
        )
        #else
        throw XCTSkip("the eui SDK is not linked (build its xcframework: exponential-ui-ffi/build-ios.sh)")
        #endif
    }

    func testSegmentCases() throws {
        let sdk = try sdk()
        for c in try cases("segmentCases") {
            let name = c["name"] as? String ?? ""
            let text = try XCTUnwrap(c["text"] as? String)
            let row = ChatExpress.expressRow(
                text, rowKey: try XCTUnwrap(c["rowKey"] as? String), final: try XCTUnwrap(c["final"] as? Bool), sdk: sdk
            )
            assertJSON(locked(row), c["expected"], name)
            XCTAssertEqual(ChatExpress.expressProse(row, text: text), c["prose"] as? String, name)
        }
    }

    func testStreamCases() throws {
        let sdk = try sdk()
        let steps = try cases("streamCases")
        let row = ChatExpress.Row(rowKey: try XCTUnwrap(steps.first?["rowKey"] as? String), sdk: sdk)
        for (index, step) in steps.enumerated() {
            let got = row.update(try XCTUnwrap(step["text"] as? String), final: try XCTUnwrap(step["final"] as? Bool))
            assertJSON(locked(got), step["expected"], "step \(index)")
            XCTAssertEqual(ChatExpress.expressProse(got, text: step["text"] as? String ?? ""), step["prose"] as? String, "step \(index)")
        }
    }

    /// Whatever the chunking, the row reaches the segmentCases states.
    func testAnyChunking() throws {
        let sdk = try sdk()
        for c in try cases("segmentCases") {
            let text = try XCTUnwrap(c["text"] as? String)
            let rowKey = try XCTUnwrap(c["rowKey"] as? String)
            let units = Array(text.utf16)
            for size in [1, 3, 17] {
                let row = ChatExpress.Row(rowKey: rowKey, sdk: sdk)
                var i = size
                while i < units.count + size {
                    // a JS slice cuts UTF-16 units: never split a pair here
                    var end = min(i, units.count)
                    if end < units.count, UTF16.isTrailSurrogate(units[end]) { end += 1 }
                    _ = row.update(String(decoding: units[0..<end], as: UTF16.self), final: false)
                    i += size
                }
                let out = row.update(text, final: try XCTUnwrap(c["final"] as? Bool))
                assertJSON(locked(out), c["expected"], "\(c["name"] ?? "") ×\(size)")
            }
        }
    }

    /// A restarted block bumps its generation; a grown one keeps it.
    func testGeneration() throws {
        let sdk = try sdk()
        let row = ChatExpress.Row(rowKey: "m:m1", sdk: sdk)
        func generation(_ segments: [ChatExpress.RowSegment]?) -> Int? {
            for case let .block(state) in segments ?? [] { return state.generation }
            return nil
        }
        XCTAssertEqual(generation(row.update("<a2ui>\nroot = Text(text=\"A", final: false)), 0)
        XCTAssertEqual(generation(row.update("<a2ui>\nroot = Text(text=\"AB\")", final: false)), 0)
        XCTAssertEqual(generation(row.update("<a2ui>\nroot = Text(text=\"C\")\n</a2ui>", final: false)), 1)
    }

    // MARK: - Actions

    func testActionCases() throws {
        for c in try cases("actionCases") {
            let name = c["name"] as? String ?? ""
            let event = try XCTUnwrap(c["event"] as? [String: Any])
            let origin = try XCTUnwrap(c["origin"] as? [String: Any])
            let got = ChatExpress.surfaceAction(
                ChatExpress.ActionEvent(
                    name: event["name"] as? String,
                    context: event["context"].flatMap(CardJSON.init(any:)),
                    userMessage: event["userMessage"] as? String
                ),
                origin: ChatExpress.ActionOrigin(
                    surfaceId: try XCTUnwrap(origin["surfaceId"] as? String),
                    block: origin["block"] as? String,
                    messageId: origin["messageId"] as? String
                )
            )
            let json: Any = switch got {
            case nil: NSNull()
            case .tooLarge: "tooLarge"
            case let .post(body, card): ["body": body, "card": card.json.any]
            }
            assertJSON(json, c["expected"], name)
        }
    }

    func testShowActionOrigin() {
        XCTAssertEqual(ChatExpress.showActionOrigin("m9"), ChatExpress.ActionOrigin(surfaceId: "show:m9", block: nil, messageId: "m9"))
    }

    func testInputCases() throws {
        for c in try cases("inputCases") {
            let name = c["name"] as? String ?? ""
            let raw = try XCTUnwrap(c["card"] as? [String: Any])
            let card = ChatExpress.ActionCard(
                name: try XCTUnwrap(raw["name"] as? String),
                context: (raw["context"] as? [String: Any]).flatMap(CardJSON.init(any:))?.objectValue,
                surfaceId: try XCTUnwrap(raw["surfaceId"] as? String),
                block: raw["block"] as? String,
                messageId: raw["messageId"] as? String
            )
            let body = try XCTUnwrap(c["body"] as? String)
            let expected = try XCTUnwrap(c["expected"] as? String)
            XCTAssertEqual(ChatExpress.actionInput(card, body: body), expected, name)
            let parsed = try XCTUnwrap(ChatExpress.parseActionInput(expected), name)
            XCTAssertEqual(parsed.card, card, name)
            XCTAssertEqual(parsed.body, body, name)
        }
        XCTAssertNil(ChatExpress.parseActionInput("[action] not json"))
        XCTAssertNil(ChatExpress.parseActionInput("Ship it"))
    }

    func testActionMarker() {
        XCTAssertEqual(ChatExpress.actionMarker(name: "save_deal", author: nil), "You pressed save_deal")
        XCTAssertEqual(ChatExpress.actionMarker(name: "approve", author: "Ana"), "Ana pressed approve")
    }

    /// A synced `action` card reads back as THE card.
    func testSyncedActionCard() throws {
        let card = TreeMessageCard(object: [
            "kind": "action", "name": "save_deal", "context": ["rep": "Ana"],
            "surfaceId": "m:m1#1", "block": "m:m1#1", "messageId": "m1",
        ])
        XCTAssertEqual(
            ChatExpress.actionCard(card),
            ChatExpress.ActionCard(name: "save_deal", context: ["rep": .string("Ana")], surfaceId: "m:m1#1", block: "m:m1#1", messageId: "m1")
        )
        XCTAssertNil(ChatExpress.actionCard(TreeMessageCard(object: ["kind": "surface"])))
    }

    // MARK: - Prose

    func testPlainProseCases() throws {
        for c in try cases("plainProseCases") {
            XCTAssertEqual(
                ChatExpress.plainProse(try XCTUnwrap(c["text"] as? String)), c["expected"] as? String, c["name"] as? String ?? ""
            )
        }
    }

    /// #EXP-2074/#EXP-2076: the specimens' sample (a subagent narration, a
    /// call turn): one block between two prose runs; its keys per lane.
    func testSpecimenSample() {
        let text = "Checked the three lanes.\n<a2ui>\nroot = Stack(children=[title, ship])\ntitle = Text(text=\"Build 1482 is green\")\nship = Button(label=\"Ship it\", on={press: Event(\"reply\", userMessage=\"Ship it\")})\n</a2ui>\nAll green."
        XCTAssertTrue(ChatExpress.hasExpressBlock(text))
        XCTAssertEqual(ChatExpress.plainProse(text), "Checked the three lanes.\n\nAll green.")
        XCTAssertEqual(ChatExpress.readText(text, authorKind: "agent"), "Checked the three lanes.\n\nAll green.")
        // A subagent lane's narration keys like the main lane's (same session).
        XCTAssertEqual(ChatExpress.feedRowKey(sessionId: "s1", messageId: "msg_sub_1", firstSeq: 9), "n:s1:msg_sub_1")
        XCTAssertEqual(ChatExpress.syncedRowKey("v5"), "m:v5")
    }

    func testPreviewCases() throws {
        for c in try cases("previewCases") {
            let name = c["name"] as? String ?? ""
            let message = TreeMessage(
                id: "m1",
                sessionId: "r",
                authorKind: try XCTUnwrap(c["authorKind"] as? String),
                authorId: nil,
                body: try XCTUnwrap(c["body"] as? String),
                card: (c["card"] as? [String: Any]).map(TreeMessageCard.init(object:)),
                answeredAt: nil,
                consumedAt: "2026-11-23T08:00:00.000Z",
                createdAt: "2026-11-23T09:00:00.000Z",
                authorSessionId: nil
            )
            let got = Chats.chatPreview(rootId: "r", messages: [message], needsYou: false, busy: false)
            assertJSON(["kind": got.kind.rawValue, "text": got.text], c["expected"], name)
        }
    }
}

/// No SDK: a row that fails the gate never asks for one.
private struct NoSdk: ExpressSdk {
    func unwrap(_ text: String) -> [ChatExpress.RawPart] {
        XCTFail("unwrap asked for a gate-less text")
        return []
    }

    func decoder(surfaceId: String) -> ExpressBlockDecoder {
        XCTFail("a decoder asked for a gate-less text")
        return Never()
    }

    private final class Never: ExpressBlockDecoder {
        func push(_ chunk: String) -> ChatExpress.Decoded { ChatExpress.Decoded(messages: [], issues: []) }
        func end() -> ChatExpress.Decoded { ChatExpress.Decoded(messages: [], issues: []) }
    }
}
