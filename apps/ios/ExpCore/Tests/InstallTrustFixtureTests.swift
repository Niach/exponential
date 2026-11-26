@testable import ExpCore
import Foundation
import XCTest

// #EXP-1950: packages/domain-contract/fixtures/install-trust.json, EVERY
// case by name: the constants, the keys and their registrations, the code
// vectors, the approval vector byte-exact, `trustCases` (reconcile), the four
// rules (`keepCases`, `sealCases`, `targetCases`, `callCases`) now AND once
// mandatory, `maliciousServer` against its rules, `newRoot`. The ed25519 runs
// through the FULL eNet binding (`EnetTrustCrypto`: `identityVerify`,
// `identitySign`, `acceptWrap`, `acceptVouch`); the same replay runs in TS
// (src/install-trust.test.ts). The pure parts (copy, the specimen rows, the
// code) replay without it.
final class InstallTrustFixtureTests: XCTestCase {
    typealias Object = [String: Any]

    static func fixture() throws -> Object {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/install-trust.json")
        return try XCTUnwrap(try JSONSerialization.jsonObject(with: try Data(contentsOf: url)) as? Object)
    }

    static func object(_ o: Object, _ key: String) throws -> Object { try XCTUnwrap(o[key] as? Object, key) }
    static func cases(_ o: Object, _ key: String) throws -> [Object] { try XCTUnwrap(o[key] as? [Object], key) }
    static func text(_ o: Object, _ key: String) throws -> String { try XCTUnwrap(o[key] as? String, key) }

    // MARK: pure (no binding)

    func testTheCopyTheIconAndTheMode() throws {
        let f = try Self.fixture()
        let copy = try Self.object(f, "copy")
        XCTAssertEqual(InstallApprovalView.Copy.newInstall, copy["newInstall"] as? String)
        XCTAssertEqual(InstallApprovalView.Copy.reset, copy["reset"] as? String)
        XCTAssertEqual(InstallApprovalView.Copy.approve, copy["approve"] as? String)
        XCTAssertEqual(InstallApprovalView.Copy.remove, copy["remove"] as? String)
        XCTAssertEqual(InstallApprovalView.Copy.waiting, copy["waiting"] as? String)
        XCTAssertEqual(InstallApprovalView.icon, f["icon"] as? String)
        let waiting = try Self.object(f, "waiting")
        XCTAssertEqual(InstallApprovalView.Waiting.topOffset, waiting["topOffset"] as? Double)
        XCTAssertEqual(InstallApprovalView.Waiting.maxWidth, waiting["maxWidth"] as? Double)
        XCTAssertEqual(InstallApprovalView.Waiting.listGap, waiting["listGap"] as? Double)
        let mode = try Self.object(f, "mode")
        XCTAssertEqual(TrustMode.build.rawValue, mode["release1"] as? String)
        XCTAssertEqual(TrustMode.mandatory.rawValue, mode["release2"] as? String)
        let approval = try Self.object(f, "approval")
        XCTAssertEqual(InstallApproval.label, approval["label"] as? String)
        XCTAssertEqual(InstallApproval.messageBytes, approval["messageBytes"] as? Int)
        let kinds = try XCTUnwrap(approval["kinds"] as? [String: Int])
        let space = try Self.object(f, "space")
        for kind in ApprovalKind.allCases {
            let want = kind.rawValue == space["kind"] as? String ? space["kindByte"] as? Int : kinds[kind.rawValue]
            XCTAssertEqual(Int(kind.byte), want, kind.rawValue)
        }
        XCTAssertEqual(kinds.count + 1, ApprovalKind.allCases.count)
    }

    private static func memberSpace() throws -> Object {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("packages/domain-contract/fixtures/member-space.json")
        return try XCTUnwrap(try JSONSerialization.jsonObject(with: try Data(contentsOf: url)) as? Object)
    }

    /// #EXP-2161 (member-space.json `rootMarkCases`).
    func testRootMarkCases() throws {
        let root = "01234567-89ab-4def-8123-456789abcdef"
        let all = try Self.cases(try Self.memberSpace(), "rootMarkCases")
        XCTAssertGreaterThanOrEqual(all.count, 10)
        for c in all {
            let name = try Self.text(c, "name")
            var store = TrustStore(selfEndpointId: "me", userId: "user-me")
            if let mark = c["mark"] as? String { store.rootMarks[root] = RootMark(rawValue: mark) }
            if c["keyed"] as? Bool == true { store.recordKey(rootId: root, epoch: 1, record: .legacy) }
            for step in try Self.cases(c, "steps") {
                let see = try XCTUnwrap(RootSight(rawValue: try Self.text(step, "see")))
                let record = step["record"] as? Bool ?? false
                XCTAssertEqual(store.ownAssistant(rootId: root.uppercased(), see: see, record: record), step["counts"] as? Bool, name)
                XCTAssertEqual(store.rootMarks[root]?.rawValue, step["mark"] as? String, name)
                if record { store.recordKey(rootId: root, epoch: 1, record: .verified) }
                store = try JSONDecoder().decode(TrustStore.self, from: try JSONEncoder().encode(store))
            }
        }
    }

    /// #EXP-2161 (web `sightOfRoot`).
    func testWhatTheRowsSayOfARoot() {
        func sight(_ agent: String?, _ own: Bool?, user: String = "me", parent: String? = nil) -> RootSight {
            sightOfRoot(parentId: parent, userId: user, agentId: agent, agentIsOwnAssistant: own, me: "me")
        }
        XCTAssertEqual(sight("a", true), .assistant)
        XCTAssertEqual(sight("later", nil), .unknown)
        for other in [sight(nil, nil), sight("t", false), sight("a", true, user: "you"), sight("a", true, parent: "p"), sight("later", nil, user: "you")] {
            XCTAssertEqual(other, .other)
        }
    }

    /// #EXP-2161 (member-space.json).
    func testMemberSpacePinCases() throws {
        let f = try Self.memberSpace()
        XCTAssertEqual(InstallTrust.spaceRegistrationLabel, try Self.text(f, "registrationLabel"))
        XCTAssertEqual(ApprovalKind.space.rawValue, try Self.text(f, "approvalKind"))
        XCTAssertEqual(Int(ApprovalKind.space.byte), f["approvalKindByte"] as? Int)
        for c in try Self.cases(f, "pinCases") {
            let name = try Self.text(c, "name")
            let pin = try (c["pin"] as? String).map { try XCTUnwrap(InstallClass(rawValue: $0)) }
            XCTAssertEqual(
                pinCovers(pin, rootIsOwnAssistant: try XCTUnwrap(c["rootIsOwnAssistant"] as? Bool)),
                try XCTUnwrap(c["expected"] as? Bool), name
            )
        }
    }

    // MARK: who makes a chat key (`keyMaker`, #EXP-2123)

    func testKeyMakerCases() throws {
        let f = try Self.object(try Self.fixture(), "keyMaker")
        XCTAssertEqual(keyMakerRefusal, try Self.text(f, "refusal"))
        XCTAssertTrue(isKeyMakerRefusal(keyMakerRefusal))
        XCTAssertFalse(isKeyMakerRefusal("Session not found"))
        XCTAssertFalse(isKeyMakerRefusal(nil))
        let server = try Self.cases(f, "serverCases")
        XCTAssertGreaterThanOrEqual(server.count, 6)
        for c in server {
            let name = try Self.text(c, "name")
            XCTAssertEqual(keyMakerWaits(
                chained: c["chained"] as? Bool ?? false, asked: c["asked"] as? Bool ?? false,
                accountChain: c["accountChain"] as? Bool ?? false, registration: c["registration"] as? Bool ?? false
            ), try XCTUnwrap(c["waits"] as? Bool), name)
        }
        let maker = try Self.cases(f, "makerCases")
        XCTAssertGreaterThanOrEqual(maker.count, 12)
        for c in maker {
            let name = try Self.text(c, "name")
            XCTAssertEqual(try keyMakerStep(
                joined: try XCTUnwrap(c["joined"] as? Bool), write: try XCTUnwrap(KeyWrite(rawValue: try Self.text(c, "write"))),
                refused: c["refused"] as? Bool ?? false
            ).rawValue, try Self.text(c, "step"), name)
        }
        let stranded = try Self.cases(f, "strandedCases")
        XCTAssertGreaterThanOrEqual(stranded.count, 7)
        for c in stranded {
            let name = try Self.text(c, "name")
            XCTAssertEqual(try strandedRotationEpoch(
                flagged: (c["flagged"] as? Int).map(UInt32.init), current: (c["current"] as? Int).map(UInt32.init),
                held: try XCTUnwrap(c["held"] as? Bool), stranded: try XCTUnwrap(c["stranded"] as? Bool),
                joined: try XCTUnwrap(c["joined"] as? Bool), left: try XCTUnwrap(c["left"] as? Bool)
            ), (c["epoch"] as? Int).map(UInt32.init), name)
        }
    }

    func testDueDecodesStranded() throws {
        let due = try JSONDecoder().decode([DueTreeKey].self, from: Data(#"[{"rootId":"a","epoch":1},{"rootId":"b","epoch":2,"stranded":true}]"#.utf8))
        XCTAssertEqual(due, [DueTreeKey(rootId: "a", epoch: 1), DueTreeKey(rootId: "b", epoch: 2, stranded: true)])
    }

    func testTheCodeVectorsAndEveryKeysCode() throws {
        let f = try Self.fixture()
        let code = try Self.object(f, "code")
        XCTAssertEqual(InstallCode.label, code["label"] as? String)
        XCTAssertEqual(String(InstallCode.alphabet), code["alphabet"] as? String)
        XCTAssertEqual(code["bits"] as? Int, 50)
        let vectors = try Self.cases(code, "vectors")
        XCTAssertEqual(vectors.count, 5)
        for v in vectors {
            XCTAssertEqual(InstallCode.of(endpointId: try Self.text(v, "endpointId")), try Self.text(v, "code"))
        }
        for (name, key) in try Self.object(f, "keys") {
            let key = try XCTUnwrap(key as? Object)
            XCTAssertEqual(InstallCode.of(endpointId: try Self.text(key, "endpointId")), try Self.text(key, "code"), name)
        }
        XCTAssertNil(InstallCode.of(endpointId: "not hex"))
        XCTAssertNil(InstallCode.of(endpointId: String(repeating: "A", count: 64)))
    }

    func testTheApprovalMessageByteExact() throws {
        let v = try Self.object(try Self.object(try Self.fixture(), "approval"), "vector")
        let subject = try Self.object(v, "subject")
        let message = try XCTUnwrap(InstallApproval.message(
            try XCTUnwrap(ApprovalKind(rawValue: try Self.text(v, "kind"))),
            userId: try Self.text(subject, "userId"), endpointId: try Self.text(subject, "endpointId"),
            x25519Public: try Self.text(subject, "x25519Public")
        ))
        XCTAssertEqual(message, try SealedContentFixtureTests.hex(v["message"]))
        XCTAssertEqual(message.count, InstallApproval.messageBytes)
        XCTAssertNil(InstallApproval.message(.approve, userId: "", endpointId: try Self.text(subject, "endpointId"), x25519Public: try Self.text(subject, "x25519Public")))
        XCTAssertNil(InstallApproval.message(.approve, userId: "u", endpointId: "zz", x25519Public: try Self.text(subject, "x25519Public")))
    }

    /// The capture specimen's rows are the fixture's keys N and T3.
    func testTheSpecimenRowsAreTheFixtures() throws {
        let keys = try Self.object(try Self.fixture(), "keys")
        for (row, name) in zip(InstallTrustSpecimenRows.rows, ["N", "T3"]) {
            let key = try Self.object(keys, name)
            XCTAssertEqual(row.endpointId, try Self.text(key, "endpointId"), name)
            XCTAssertEqual(row.userId, try Self.text(key, "userId"), name)
            XCTAssertEqual(row.label, try Self.text(key, "label"), name)
            XCTAssertEqual(row.code, try Self.text(key, "code"), name)
            XCTAssertEqual(row.code, InstallCode.of(endpointId: row.endpointId), name)
        }
    }

    /// The trees web draws (install-approval.tsx): ids, components, props.
    func testTheTreesAreWebs() throws {
        let row = InstallTrustSpecimenRows.rows[1]
        let tree = InstallApprovalView.pendingTree([(row, "Mira's new iPhone")])
        let json = try XCTUnwrap(try JSONSerialization.jsonObject(with: Data(tree.compactString.utf8)) as? Object)
        XCTAssertEqual(json["id"] as? String, "pending-installs")
        XCTAssertEqual(json["component"] as? String, "Section")
        let item = try XCTUnwrap((json["children"] as? [Object])?.first)
        XCTAssertEqual(item["id"] as? String, "install-install-T3")
        XCTAssertEqual(item["component"] as? String, "Stack")
        XCTAssertEqual(item["props"] as? [String: String], ["direction": "vertical", "gap": "sm"])
        let lines = try XCTUnwrap(item["children"] as? [Object])
        XCTAssertEqual(lines.map { $0["component"] as? String }, ["Row", "Stack"])
        let r = lines[0]
        XCTAssertEqual(r["id"] as? String, "install-install-T3-row")
        XCTAssertEqual(r["props"] as? [String: String], ["title": "Mira's new iPhone", "subtitle": "776AF T3NNF"])
        // The Row's trailing slot is the Badge alone: no Button beside the code.
        let slots = try XCTUnwrap(r["slots"] as? Object)
        XCTAssertEqual(Array(slots.keys), ["trailing"])
        let badge = try XCTUnwrap(slots["trailing"] as? Object)
        XCTAssertEqual(badge["component"] as? String, "Badge")
        XCTAssertNil(badge["children"])
        XCTAssertEqual(badge["props"] as? [String: String], ["text": "Reset", "variant": "secondary"])
        XCTAssertFalse(String(data: try JSONSerialization.data(withJSONObject: r), encoding: .utf8)!.contains("\"Button\""))
        // Under it the actions, Approve then Remove.
        let actions = lines[1]
        XCTAssertEqual(actions["props"] as? [String: String], ["direction": "horizontal", "gap": "sm"])
        XCTAssertEqual(actions["style"] as? [String: String], ["paddingHorizontal": "$spacing.md", "paddingBottom": "$spacing.md"])
        let parts = try XCTUnwrap(actions["children"] as? [Object])
        XCTAssertEqual(parts.map { $0["component"] as? String }, ["Button", "Button"])
        XCTAssertEqual(parts[0]["props"] as? [String: String], ["label": "Approve", "size": "sm"])
        XCTAssertEqual(parts[1]["props"] as? [String: String], ["label": "Remove", "size": "sm", "variant": "ghost"])
        let waiting = try XCTUnwrap(try JSONSerialization.jsonObject(with: Data(InstallApprovalView.waitingTree(code: "HNMBN A0319").compactString.utf8)) as? Object)
        XCTAssertEqual(waiting["component"] as? String, "EmptyState")
        XCTAssertEqual(waiting["props"] as? [String: String], ["icon": "ui-private", "title": "Waiting for approval", "description": "HNMBN A0319"])
        // A row's title: the label; a reset leads with the member's name; nothing = the code.
        XCTAssertEqual(InstallApprovalView.title(row, memberName: "Mira"), "Mira · Mira's new iPhone")
        XCTAssertEqual(InstallApprovalView.title(TrustRow(kind: .newInstall, installId: "x", endpointId: row.endpointId, userId: "u", label: nil, code: row.code), memberName: nil), row.code)
    }

    func testTheNewRootRule() throws {
        let block = try Self.object(try Self.fixture(), "newRoot")
        XCTAssertEqual(block["makerRecord"] as? String, KeyRecord.verified.rawValue)
        XCTAssertEqual(block["readerRecordBeforeResign"] as? [String: String], ["now": "unsigned", "mandatory": "refused"])
        XCTAssertEqual(block["readerRecordAfterResign"] as? String, "verified")
    }

    #if ENET_NODE
    private let crypto = EnetTrustCrypto()
    private var fx: TrustFixture { get throws { try TrustFixture() } }
    private func keys() throws -> TrustFixture.Keys { try fx.keys }
    private func served(_ spec: Object, _ keys: TrustFixture.Keys) throws -> ServedInstall { try fx.served(spec) }
    private func store(_ spec: Object, _ keys: TrustFixture.Keys) throws -> TrustStore { try fx.store(spec) }
    private func namedStore(_ name: String) throws -> (store: TrustStore, identity: Data, installSecret: Data) { try fx.namedStore(name) }

    /// #EXP-2161: a server that stops serving a space (or its registration)
    /// does not make it an unpinned ordinary install.
    func testOnceASpaceAlwaysASpace() throws {
        let fx = try self.fx
        let s = try fx.keys.endpoint("S")
        func seen(_ store: TrustStore, _ specs: [Object]) throws -> TrustStore {
            InstallTrust.reconcile(store, served: TrustAnswer(installs: try specs.map { try fx.served($0) }, approvals: []), crypto: crypto).store
        }
        let a: Object = ["key": "A"]
        let bare: Object = ["key": "S", "registration": "none"]
        let (joined, identity, secret) = try fx.namedStore("withoutSpace", inSpace: true)
        let known = try seen(joined, [a, ["key": "S"]])
        XCTAssertTrue(known.knownSpaces.contains(s))
        for specs in [[a], [a, bare]] {
            var store = try seen(known, specs)
            XCTAssertTrue(store.knownSpaces.contains(s))
            XCTAssertNil(store.pins[s])
            store.existing = true
            store.joined = false
            XCTAssertNil(try seen(store, specs).pins[s])
        }
        let keep = try XCTUnwrap(try Self.cases(fx.space, "keepCases").first { $0["store"] as? String == "withoutSpace" })
        let w = try Self.object(keep, "wrap")
        let wrap = HeldTreeKey(
            rootId: try Self.text(w, "rootId"), epoch: UInt32(try XCTUnwrap(w["epoch"] as? Int)), wrapped: try Self.text(w, "wrapped"),
            signature: w["signature"] as? String, signerEndpointId: w["signerEndpointId"] as? String
        )
        func kept(_ store: TrustStore) -> InstallTrust.KeepVerdict {
            InstallTrust.keepWrap(wrap, identity: identity, installSecret: secret, store: store, mode: .record, crypto: crypto, rootIsOwnAssistant: true)
        }
        if case .refuse = kept(joined) { XCTFail("kept before it was known") }
        let gone = try seen(known, [a])
        guard case .refuse(.unpinned) = kept(gone) else { return XCTFail("a known space's wrap") }
        let c = try XCTUnwrap(try Self.cases(fx.space, "sealCases").first { $0["store"] as? String == "withoutSpace" })
        let v = try Self.object(c, "vouch")
        let vouch = InstallTrust.Vouch(
            recipientEndpointId: try Self.text(v, "recipientEndpointId"), wrapped: try Self.text(v, "wrapped"),
            signature: try Self.text(v, "signature"), signerEndpointId: try Self.text(v, "signerEndpointId")
        )
        XCTAssertEqual(
            InstallTrust.sealTo(
                rootId: try Self.text(c, "rootId"), epoch: 1, publicKey: try Self.text(c, "publicKey"), held: nil, vouch: vouch,
                store: gone, mode: .record, crypto: crypto, rootIsOwnAssistant: true
            ),
            .refuse(.unpinned)
        )
        XCTAssertEqual(
            InstallTrust.wrapTarget(try fx.served(bare), store: gone, mode: .record, crypto: crypto, rootIsOwnAssistant: true),
            .refuse(.unapproved)
        )
    }

    private func at(_ c: Object, _ mode: TrustMode) throws -> Object {
        try Self.object(c, mode == .record ? "now" : "mandatory")
    }

    // MARK: the keys

    func testEveryKeyAndItsRegistration() throws {
        let keys = try keys()
        for name in keys.all.keys.sorted() {
            XCTAssertEqual(try crypto.endpointId(identity: try keys.identity(name)), try keys.endpoint(name), name)
            XCTAssertEqual(try EnetSealedCrypto().installKey(identity: try keys.identity(name)).publicKey, try keys.text(name, "x25519Public"), name)
            let install = try served(["key": name], keys)
            XCTAssertTrue(InstallTrust.registrationValid(install, crypto: crypto), name)
            XCTAssertEqual(install.registrationSignature, try crypto.sign(identity: try keys.identity(name), message: Data(InstallRegistration.message(
                userId: install.userId, endpointId: install.endpointId, x25519Public: install.x25519Public, signedAt: try keys.text(name, "signedAt")
            ).utf8)), name)
            XCTAssertEqual(InstallTrust.registrationValid(try served(["key": name, "registration": "forged"], keys), crypto: crypto), name == "Z", name)
            XCTAssertFalse(InstallTrust.registrationValid(try served(["key": name, "registration": "none"], keys), crypto: crypto), name)
            // Another X25519 key than the registration binds.
            XCTAssertFalse(InstallTrust.registrationValid(try served(["key": name, "x25519Public": try keys.text(name == "Z" ? "A" : "Z", "x25519Public")], keys), crypto: crypto), name)
        }
    }

    func testTheApprovalVectorSignsAndVerifies() throws {
        let f = try Self.fixture()
        let keys = try keys()
        let v = try Self.object(try Self.object(f, "approval"), "vector")
        let s = try Self.object(v, "subject")
        let subject = ServedInstall(
            installId: "x", userId: try Self.text(s, "userId"), endpointId: try Self.text(s, "endpointId"), x25519Public: try Self.text(s, "x25519Public"),
            registrationSignature: nil, registrationSignedAt: nil, approvalRequested: false
        )
        let signature = try Self.text(v, "signature")
        let approver = try Self.text(v, "approverEndpointId")
        XCTAssertEqual(try InstallApproval.sign(.approve, subject: subject, identity: try keys.identity("A"), crypto: crypto), signature)
        XCTAssertTrue(InstallApproval.verify(.approve, subject: subject, signature: signature, approverEndpointId: approver, crypto: crypto))
        // Every field is covered: another kind, user, endpoint or key fails.
        XCTAssertFalse(InstallApproval.verify(.reset, subject: subject, signature: signature, approverEndpointId: approver, crypto: crypto))
        var other = subject
        other.userId = "user-other"
        XCTAssertFalse(InstallApproval.verify(.approve, subject: other, signature: signature, approverEndpointId: approver, crypto: crypto))
        other = subject
        other.endpointId = try keys.endpoint("B")
        XCTAssertFalse(InstallApproval.verify(.approve, subject: other, signature: signature, approverEndpointId: approver, crypto: crypto))
        other = subject
        other.x25519Public = try keys.text("B", "x25519Public")
        XCTAssertFalse(InstallApproval.verify(.approve, subject: other, signature: signature, approverEndpointId: approver, crypto: crypto))
        XCTAssertFalse(InstallApproval.verify(.approve, subject: subject, signature: signature, approverEndpointId: try keys.endpoint("B"), crypto: crypto))
        // A malformed signature never verifies (and never throws).
        XCTAssertFalse(crypto.verify(signature: String(signature.dropLast()), message: Data(), endpointId: approver))
        XCTAssertFalse(crypto.verify(signature: signature, message: Data(), endpointId: approver.uppercased()))
    }

    // MARK: reconcile

    func testReplaysEveryTrustCase() throws {
        let f = try Self.fixture()
        let keys = try keys()
        let all = try Self.cases(f, "trustCases")
        XCTAssertEqual(all.count, 34)
        for c in all {
            let name = try Self.text(c, "name")
            let answer = TrustAnswer(
                installs: try Self.cases(c, "installs").map { try served($0, keys) },
                approvals: try Self.cases(c, "approvals").map { a in
                    ServedApproval(
                        kind: try XCTUnwrap(ApprovalKind(rawValue: try Self.text(a, "kind"))),
                        subjectEndpointId: try keys.endpoint(try Self.text(a, "subject")),
                        approverEndpointId: try keys.endpoint(try Self.text(a, "approver")),
                        signature: try Self.text(a, "signature")
                    )
                }
            )
            let view = InstallTrust.reconcile(try store(try Self.object(c, "before"), keys), served: answer, crypto: crypto)
            XCTAssertEqual(view.status.rawValue, try Self.text(c, "status"), name)
            let after = try Self.object(c, "after")
            XCTAssertEqual(view.store.joined, after["joined"] as? Bool, name)
            var pins: [String: [String?]] = [:]
            for (endpointId, pin) in view.store.pins {
                pins[try XCTUnwrap(keys.name(of: endpointId), name)] = [pin.via.rawValue, pin.by.flatMap(keys.name(of:))]
            }
            let expected = try XCTUnwrap(after["pins"] as? [String: [Any]]).mapValues { $0.map { $0 as? String } }
            XCTAssertEqual(pins, expected, name)
            XCTAssertEqual(view.store.members, after["members"] as? [String], name)
            XCTAssertEqual(view.store.declined.map { keys.name(of: $0) ?? $0 }, after["declined"] as? [String], name)
            let rows = try Self.cases(c, "rows").map { r in
                [try Self.text(r, "kind"), try Self.text(r, "key"), try Self.text(r, "installId"), try Self.text(r, "code"), r["label"] as? String ?? "<nil>"]
            }
            XCTAssertEqual(view.rows.map { [$0.kind.rawValue, keys.name(of: $0.endpointId) ?? $0.endpointId, $0.installId, $0.code, $0.label ?? "<nil>"] }, rows, name)
            XCTAssertEqual(view.approveExisting.map { keys.name(of: $0) ?? $0 }, c["approveExisting"] as? [String] ?? [], name)
            if let existing = after["existing"] as? Bool { XCTAssertEqual(view.store.existing, existing, name) }
        }
    }

    /// Decision (e): on day one no row and no waiting state for an account
    /// whose installs all existed.
    func testDayOneNoRowNoWaiting() throws {
        let dayOne = try Self.cases(try Self.fixture(), "trustCases").filter { $0["dayOne"] as? Bool == true }
        XCTAssertGreaterThanOrEqual(dayOne.count, 4)
        for c in dayOne {
            let name = try Self.text(c, "name")
            XCTAssertEqual(try Self.cases(c, "rows").count, 0, name)
            XCTAssertNotEqual(try Self.text(c, "status"), TrustStatus.waiting.rawValue, name)
            XCTAssertEqual(try Self.object(c, "after")["joined"] as? Bool, true, name)
        }
    }

    /// Decision (e): a planted install after the first reconcile is never pinned.
    func testAPlantedInstallAfterTheFirstReconcileIsNeverPinned() throws {
        let attacks = try Self.cases(try Self.fixture(), "trustCases").filter { $0["attack"] as? String == "existing" }
        XCTAssertGreaterThanOrEqual(attacks.count, 3)
        for c in attacks {
            let name = try Self.text(c, "name")
            let before = try Self.object(c, "before")
            if before["existing"] as? Bool == true { continue } // the accepted moment: the first reconcile
            let pins = try XCTUnwrap(try Self.object(c, "after")["pins"] as? Object).keys.sorted()
            XCTAssertFalse(pins.contains("Z"), name)
            XCTAssertEqual(pins, try XCTUnwrap(before["pins"] as? Object).keys.sorted(), name)
        }
        // Its row: a New install (with a registration that binds its key).
        XCTAssertTrue(try attacks.contains { c in try Self.cases(c, "rows").contains { $0["key"] as? String == "Z" && $0["kind"] as? String == "newInstall" } })
    }

    // MARK: member spaces (the `space` section, #EXP-2161)

    func testSpaceConstantsRegistrationsAndTheApproval() throws {
        let fx = try self.fx
        let keys = fx.keys
        XCTAssertEqual(InstallTrust.spaceRegistrationLabel, try Self.text(fx.space, "registrationLabel"))
        for name in keys.space.keys.sorted() {
            let message = InstallTrust.spaceRegistrationMessage(
                userId: try keys.text(name, "userId"), endpointId: try keys.endpoint(name),
                x25519Public: try keys.text(name, "x25519Public"), signedAt: try keys.text(name, "signedAt")
            )
            XCTAssertEqual(try crypto.sign(identity: try keys.identity(name), message: Data(message.utf8)), try keys.text(name, "registrationSignature"), name)
            XCTAssertEqual(InstallCode.of(endpointId: try keys.endpoint(name)), try keys.text(name, "code"), name)
            XCTAssertEqual(InstallTrust.registrationClass(try fx.served(["key": name]), crypto: crypto), .space, name)
            XCTAssertFalse(InstallTrust.registrationValid(try fx.served(["key": name]), crypto: crypto), name)
        }
        XCTAssertEqual(InstallTrust.registrationClass(try fx.served(["key": "A"]), crypto: crypto), .install)
        XCTAssertNil(InstallTrust.registrationClass(try fx.served(["key": "S", "registration": "none"]), crypto: crypto))
        let v = try Self.object(fx.space, "approval")
        let s = try Self.object(v, "subject")
        let subject = ServedInstall(
            installId: "x", userId: try Self.text(s, "userId"), endpointId: try Self.text(s, "endpointId"),
            x25519Public: try Self.text(s, "x25519Public"), registrationSignature: nil, registrationSignedAt: nil, approvalRequested: false
        )
        let signature = try Self.text(v, "signature")
        let approver = try Self.text(v, "approverEndpointId")
        XCTAssertEqual(InstallApproval.message(.space, subject: subject), try SealedContentFixtureTests.hex(v["message"]))
        XCTAssertEqual(try InstallApproval.sign(.space, subject: subject, identity: try keys.identity("A"), crypto: crypto), signature)
        XCTAssertTrue(InstallApproval.verify(.space, subject: subject, signature: signature, approverEndpointId: approver, crypto: crypto))
        XCTAssertFalse(InstallApproval.verify(.approve, subject: subject, signature: signature, approverEndpointId: approver, crypto: crypto))
    }

    func testSpaceTrustCases() throws {
        let fx = try self.fx
        let keys = fx.keys
        let all = try Self.cases(fx.space, "trustCases")
        XCTAssertFalse(all.isEmpty)
        for c in all {
            let name = try Self.text(c, "name")
            let answer = TrustAnswer(
                installs: try Self.cases(c, "installs").map { try fx.served($0) },
                approvals: try Self.cases(c, "approvals").map { a in
                    ServedApproval(
                        kind: try XCTUnwrap(ApprovalKind(rawValue: try Self.text(a, "kind"))),
                        subjectEndpointId: try keys.endpoint(try Self.text(a, "subject")),
                        approverEndpointId: try keys.endpoint(try Self.text(a, "approver")),
                        signature: try Self.text(a, "signature")
                    )
                }
            )
            let view = InstallTrust.reconcile(try fx.store(try Self.object(c, "before")), served: answer, crypto: crypto)
            XCTAssertEqual(view.status.rawValue, try Self.text(c, "status"), name)
            let after = try Self.object(c, "after")
            XCTAssertEqual(view.store.joined, after["joined"] as? Bool, name)
            var pins: [String: [String]] = [:]
            for (endpointId, pin) in view.store.pins {
                pins[try XCTUnwrap(keys.name(of: endpointId), name)] =
                    [pin.via.rawValue, pin.by.flatMap(keys.name(of:)) ?? "-"] + (pin.installClass == .space ? ["space"] : [])
            }
            let expected = try XCTUnwrap(after["pins"] as? [String: [Any]]).mapValues { $0.map { $0 as? String ?? "-" } }
            XCTAssertEqual(pins, expected, name)
            XCTAssertEqual(view.store.members, after["members"] as? [String], name)
            XCTAssertEqual(
                view.rows.map { [$0.kind.rawValue, keys.name(of: $0.endpointId) ?? $0.endpointId] },
                try Self.cases(c, "rows").map { [try Self.text($0, "kind"), try Self.text($0, "key")] }, name
            )
            if let known = after["knownSpaces"] as? [String] { XCTAssertEqual(view.store.knownSpaces.map { keys.name(of: $0) ?? $0 }, known, name) }
            if view.store.space { XCTAssertEqual(view.approveExisting, [], name) }
        }
    }

    func testSpaceRulesCoverTheMembersOwnAssistantAlone() throws {
        let fx = try self.fx
        for mode in TrustMode.allCases {
            for c in try Self.cases(fx.space, "keepCases") {
                let label = "\(try Self.text(c, "name")) (\(mode))"
                let want = try at(c, mode)
                let (store, identity, secret) = try fx.namedStore(try Self.text(c, "store"), inSpace: true)
                let w = try Self.object(c, "wrap")
                let wrap = HeldTreeKey(
                    rootId: try Self.text(w, "rootId"), epoch: UInt32(try XCTUnwrap(w["epoch"] as? Int)), wrapped: try Self.text(w, "wrapped"),
                    signature: w["signature"] as? String, signerEndpointId: w["signerEndpointId"] as? String
                )
                let own = try XCTUnwrap(c["rootIsOwnAssistant"] as? Bool)
                switch InstallTrust.keepWrap(
                    wrap, identity: identity, installSecret: secret, store: store, mode: mode, crypto: crypto, rootIsOwnAssistant: own
                ) {
                case let .keep(record, _, _):
                    XCTAssertEqual(want["keep"] as? Bool, true, label)
                    XCTAssertEqual(record.rawValue, want["record"] as? String, label)
                case let .refuse(reason):
                    XCTAssertEqual(want["keep"] as? Bool, false, label)
                    XCTAssertEqual(reason.rawValue, want["reason"] as? String, label)
                }
            }
            for c in try Self.cases(fx.space, "sealCases") {
                let label = "\(try Self.text(c, "name")) (\(mode))"
                let want = try at(c, mode)
                let store = try fx.namedStore(try Self.text(c, "store"), inSpace: true).store
                let own = try XCTUnwrap(c["rootIsOwnAssistant"] as? Bool)
                let vouch = try (c["vouch"] as? Object).map { v in
                    InstallTrust.Vouch(
                        recipientEndpointId: try Self.text(v, "recipientEndpointId"), wrapped: try Self.text(v, "wrapped"),
                        signature: try Self.text(v, "signature"), signerEndpointId: try Self.text(v, "signerEndpointId")
                    )
                }
                let got = InstallTrust.sealTo(
                    rootId: try Self.text(c, "rootId"), epoch: UInt32(try XCTUnwrap(c["epoch"] as? Int)), publicKey: try Self.text(c, "publicKey"),
                    held: nil, vouch: vouch, store: store, mode: mode, crypto: crypto, rootIsOwnAssistant: own
                )
                switch got {
                case let .seal(record):
                    XCTAssertEqual(want["seal"] as? Bool, true, label)
                    XCTAssertEqual(record.rawValue, want["record"] as? String, label)
                case let .refuse(reason):
                    XCTAssertEqual(want["seal"] as? Bool, false, label)
                    XCTAssertEqual(reason.rawValue, want["reason"] as? String, label)
                }
            }
            for c in try Self.cases(fx.space, "targetCases") {
                let label = "\(try Self.text(c, "name")) (\(mode))"
                let want = try at(c, mode)
                let store = try fx.namedStore(try Self.text(c, "store"), inSpace: true).store
                let own = try XCTUnwrap(c["rootIsOwnAssistant"] as? Bool)
                let target = try fx.served(try Self.object(c, "target"))
                switch InstallTrust.wrapTarget(target, store: store, mode: mode, crypto: crypto, rootIsOwnAssistant: own) {
                case let .wrap(record):
                    XCTAssertEqual(want["wrap"] as? Bool, true, label)
                    XCTAssertEqual(record.rawValue, want["record"] as? String, label)
                case let .refuse(reason):
                    XCTAssertEqual(want["wrap"] as? Bool, false, label)
                    XCTAssertEqual(reason.rawValue, want["reason"] as? String, label)
                }
            }
        }
    }

    // MARK: the four rules, now and once mandatory

    func testR1KeepWrapEveryCase() throws {
        let all = try Self.cases(try Self.object(try Self.fixture(), "rules"), "keepCases")
        XCTAssertEqual(all.count, 9)
        for c in all {
            let name = try Self.text(c, "name")
            let (store, identity, secret) = try namedStore(try Self.text(c, "store"))
            let w = try Self.object(c, "wrap")
            let wrap = HeldTreeKey(
                rootId: try Self.text(w, "rootId"), epoch: UInt32(try XCTUnwrap(w["epoch"] as? Int)), wrapped: try Self.text(w, "wrapped"),
                signature: w["signature"] as? String, signerEndpointId: w["signerEndpointId"] as? String
            )
            for mode in TrustMode.allCases {
                let want = try at(c, mode)
                switch InstallTrust.keepWrap(wrap, identity: identity, installSecret: secret, store: store, mode: mode, crypto: crypto) {
                case let .keep(record, _, _):
                    XCTAssertEqual(want["keep"] as? Bool, true, "\(name) (\(mode))")
                    XCTAssertEqual(record.rawValue, want["record"] as? String, "\(name) (\(mode))")
                case let .refuse(reason):
                    XCTAssertEqual(want["keep"] as? Bool, false, "\(name) (\(mode))")
                    XCTAssertEqual(reason.rawValue, want["reason"] as? String, "\(name) (\(mode))")
                }
            }
        }
    }

    func testR2R3SealToEveryCase() throws {
        let all = try Self.cases(try Self.object(try Self.fixture(), "rules"), "sealCases")
        XCTAssertEqual(all.count, 11)
        for c in all {
            let name = try Self.text(c, "name")
            let store = try namedStore(try Self.text(c, "store")).store
            let held = try (c["held"] as? Object).map { h in
                (secret: try SealedContentFixtureTests.hex(h["secret"]), record: try XCTUnwrap(KeyRecord(rawValue: try Self.text(h, "record"))))
            }
            let vouch = try (c["vouch"] as? Object).map { v in
                InstallTrust.Vouch(
                    recipientEndpointId: try Self.text(v, "recipientEndpointId"), wrapped: try Self.text(v, "wrapped"),
                    signature: try Self.text(v, "signature"), signerEndpointId: try Self.text(v, "signerEndpointId")
                )
            }
            for mode in TrustMode.allCases {
                let want = try at(c, mode)
                let got = InstallTrust.sealTo(
                    rootId: try Self.text(c, "rootId"), epoch: UInt32(try XCTUnwrap(c["epoch"] as? Int)), publicKey: try Self.text(c, "publicKey"),
                    held: held, vouch: vouch, store: store, mode: mode, crypto: crypto
                )
                switch got {
                case let .seal(record):
                    XCTAssertEqual(want["seal"] as? Bool, true, "\(name) (\(mode))")
                    XCTAssertEqual(record.rawValue, want["record"] as? String, "\(name) (\(mode))")
                case let .refuse(reason):
                    XCTAssertEqual(want["seal"] as? Bool, false, "\(name) (\(mode))")
                    XCTAssertEqual(reason.rawValue, want["reason"] as? String, "\(name) (\(mode))")
                }
            }
        }
    }

    func testR4WrapTargetEveryCase() throws {
        let keys = try keys()
        let all = try Self.cases(try Self.object(try Self.fixture(), "rules"), "targetCases")
        XCTAssertEqual(all.count, 14)
        // R4 runs against every named store (`adopted` = one that pinned its account's installs `existing`).
        XCTAssertEqual(Set(all.compactMap { $0["store"] as? String }), ["victim", "waiting", "adopted"])
        for c in all {
            let name = try Self.text(c, "name")
            let store = try namedStore(try Self.text(c, "store")).store
            let roots = Set(try XCTUnwrap(c["roots"] as? [String]).map { try keys.endpoint($0) })
            let target = try served(try Self.object(c, "target"), keys)
            for mode in TrustMode.allCases {
                let want = try at(c, mode)
                switch InstallTrust.wrapTarget(target, store: store, mode: mode, roots: roots, crypto: crypto) {
                case let .wrap(record):
                    XCTAssertEqual(want["wrap"] as? Bool, true, "\(name) (\(mode))")
                    XCTAssertEqual(record.rawValue, want["record"] as? String, "\(name) (\(mode))")
                case let .refuse(reason):
                    XCTAssertEqual(want["wrap"] as? Bool, false, "\(name) (\(mode))")
                    XCTAssertEqual(reason.rawValue, want["reason"] as? String, "\(name) (\(mode))")
                }
            }
        }
    }

    func testCallKeyEveryCase() throws {
        let all = try Self.cases(try Self.object(try Self.fixture(), "rules"), "callCases")
        XCTAssertEqual(all.count, 6)
        for c in all {
            let name = try Self.text(c, "name")
            let store = try namedStore(try Self.text(c, "store")).store
            let target = try Self.object(c, "target")
            let held = try (c["held"] as? Object).map { h in
                (secret: try SealedContentFixtureTests.hex(h["secret"]), record: try XCTUnwrap(KeyRecord(rawValue: try Self.text(h, "record"))))
            }
            for mode in TrustMode.allCases {
                let want = try at(c, mode)
                switch InstallTrust.callKey(rootId: try Self.text(target, "rootId"), epoch: UInt32(try XCTUnwrap(target["epoch"] as? Int)), held: held, store: store, mode: mode) {
                case let .key(secret, record):
                    XCTAssertEqual(want["key"] as? Bool, true, "\(name) (\(mode))")
                    XCTAssertEqual(record.rawValue, want["record"] as? String, "\(name) (\(mode))")
                    XCTAssertEqual(secret, held?.secret, name)
                case .noKey:
                    XCTAssertEqual(want["noKey"] as? String, "noKey", "\(name) (\(mode))")
                case .rollback:
                    XCTAssertEqual(want["noKey"] as? String, "rollback", "\(name) (\(mode))")
                }
            }
        }
    }

    /// Every attack case names its rule's case of the same name with the same
    /// verdicts, and once mandatory none keeps, seals, wraps or keys anything.
    func testMaliciousServerCasesAreTheirRulesCases() throws {
        let f = try Self.fixture()
        let rules = try Self.object(f, "rules")
        let entries = try Self.cases(f, "maliciousServer")
        XCTAssertEqual(entries.map { $0["attack"] as? Int }, [1, 2, 3, 4])
        var count = 0
        for entry in entries {
            for c in try Self.cases(entry, "cases") {
                count += 1
                let name = try Self.text(c, "name")
                let source = try XCTUnwrap(try Self.cases(rules, try Self.text(c, "replay")).first { $0["name"] as? String == name }, name)
                XCTAssertEqual(source["attack"] as? Int, entry["attack"] as? Int, name)
                XCTAssertEqual(c["now"] as? NSDictionary, source["now"] as? NSDictionary, name)
                XCTAssertEqual(c["mandatory"] as? NSDictionary, source["mandatory"] as? NSDictionary, name)
                XCTAssertFalse(try Self.object(c, "answers").isEmpty, name)
                let mandatory = try Self.object(c, "mandatory")
                XCTAssertEqual((mandatory["keep"] ?? mandatory["seal"] ?? mandatory["wrap"] ?? mandatory["key"]) as? Bool, false, name)
            }
        }
        XCTAssertGreaterThan(count, 0)
    }
    #else
    func testTheBindingIsBuilt() throws {
        throw XCTSkip("the full ExponentialNetFFI.xcframework was not built (build-ios.sh)")
    }
    #endif
}

#if ENET_NODE
/// The fixture's inputs, built the way src/install-trust.test.ts builds them
/// (the replay and the malicious-server suite share them).
struct TrustFixture {
    typealias Object = [String: Any]

    struct Keys {
        let all: Object

        /// #EXP-2161: the `space` section's keys (S, X).
        let space: Object

        func key(_ name: String) throws -> Object { try XCTUnwrap((all[name] ?? space[name]) as? Object, name) }
        func endpoint(_ name: String) throws -> String { try text(name, "endpointId") }
        func text(_ name: String, _ field: String) throws -> String { try InstallTrustFixtureTests.text(try key(name), field) }
        func identity(_ name: String) throws -> Data { try SealedContentFixtureTests.hex(try key(name)["identity"]) }
        func name(of endpointId: String) -> String? {
            all.merging(space) { first, _ in first }.first { ($0.value as? Object)?["endpointId"] as? String == endpointId }?.key
        }
    }

    let f: Object
    let space: Object
    let keys: Keys
    let crypto = EnetTrustCrypto()

    init() throws {
        f = try InstallTrustFixtureTests.fixture()
        space = try InstallTrustFixtureTests.object(f, "space")
        keys = Keys(all: try InstallTrustFixtureTests.object(f, "keys"), space: try InstallTrustFixtureTests.object(space, "keys"))
    }

    /// An install spec (`key`, `approvalRequested`, `registration`, `x25519Public`) as served.
    func served(_ spec: Object) throws -> ServedInstall {
        let name = try InstallTrustFixtureTests.text(spec, "key")
        let key = try keys.key(name)
        let registration = spec["registration"] as? String
        let signedAt = try keys.text(name, "signedAt")
        let x25519 = try keys.text(name, "x25519Public")
        var signature: String? = try keys.text(name, "registrationSignature")
        if registration == "none" { signature = nil }
        if registration == "forged" {
            // The server's install Z signed the registration message.
            let message = InstallRegistration.message(userId: try keys.text(name, "userId"), endpointId: try keys.endpoint(name), x25519Public: x25519, signedAt: signedAt)
            signature = try crypto.sign(identity: try keys.identity("Z"), message: Data(message.utf8))
        }
        return ServedInstall(
            installId: "install-\(name)",
            userId: try keys.text(name, "userId"),
            endpointId: try keys.endpoint(name),
            x25519Public: spec["x25519Public"] as? String ?? x25519,
            registrationSignature: signature,
            registrationSignedAt: registration == "none" ? nil : signedAt,
            approvalRequested: spec["approvalRequested"] as? Bool ?? false,
            label: key["label"] as? String
        )
    }

    /// A store spec (`viewer`, `joined`, `existing`, `pins`, `members`, `declined`, `roots`).
    func store(_ spec: Object) throws -> TrustStore {
        let viewer = try InstallTrustFixtureTests.text(spec, "viewer")
        var store = TrustStore(
            selfEndpointId: try keys.endpoint(viewer), userId: try keys.text(viewer, "userId"), existing: spec["existing"] as? Bool ?? false
        )
        store.joined = try XCTUnwrap(spec["joined"] as? Bool)
        store.space = spec["space"] as? Bool ?? false
        for (name, pin) in try XCTUnwrap(spec["pins"] as? Object) {
            let pair = try XCTUnwrap(pin as? [Any])
            let via = try XCTUnwrap(PinVia(rawValue: try XCTUnwrap(pair[0] as? String)))
            store.pins[try keys.endpoint(name)] = TrustPin(
                userId: try keys.text(name, "userId"), x25519Public: try keys.text(name, "x25519Public"),
                via: via, by: try (pair[1] as? String).map { try keys.endpoint($0) },
                installClass: (pair.count > 2 ? pair[2] as? String : nil).flatMap(InstallClass.init(rawValue:)) ?? .install
            )
        }
        store.members = spec["members"] as? [String] ?? []
        store.knownSpaces = try (spec["knownSpaces"] as? [String] ?? []).map { try keys.endpoint($0) }
        store.declined = try (spec["declined"] as? [String] ?? []).map { try keys.endpoint($0) }
        if let roots = spec["roots"] as? Object {
            for (root, value) in roots {
                let value = try XCTUnwrap(value as? Object)
                let epochs = try XCTUnwrap(value["epochs"] as? [String: String]).compactMapValues(KeyRecord.init(rawValue:))
                store.roots[root] = TrustRootRecord(verifiedEpoch: UInt32(try XCTUnwrap(value["verifiedEpoch"] as? Int)), epochs: epochs)
            }
        }
        return store
    }

    func namedStore(_ name: String, inSpace: Bool = false) throws -> (store: TrustStore, identity: Data, installSecret: Data) {
        let spec = try InstallTrustFixtureTests.object(try InstallTrustFixtureTests.object(inSpace ? space : f, "stores"), name)
        let viewer = try InstallTrustFixtureTests.text(spec, "viewer")
        return (try store(spec), try keys.identity(viewer), try SealedContentFixtureTests.hex(try keys.key(viewer)["installSecret"]))
    }
}
#endif

