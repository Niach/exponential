@testable import ExpCore
import Foundation
import XCTest

// #EXP-1950: every tree key wrap the iOS holder MAKES is signed by its
// install identity: the first key, a rotation, the pending answers and the
// signing again of decision (c) (a new root's inline seal, keys held from
// before). The fake key API stores and verifies like the server
// (apps/web/src/lib/sessions/wrap-signatures.ts): a signed wrap must be the
// caller's install's signature over the root, the epoch, the STORED tree
// public key, the recipient's endpoint id and the wrapped bytes, through the
// binding's `verifyWrapSignature` (fixtures/wrap-signature.json).
final class WrapSigningTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appendingPathComponent("packages/domain-contract/fixtures/wrap-signature.json")
        return try XCTUnwrap(try JSONSerialization.jsonObject(with: try Data(contentsOf: url)) as? [String: Any])
    }

    /// A holder signs over the key of the secret it holds (no binding needed).
    func testTheTreePublicKeyOfAHeldSecretIsTheFixtures() throws {
        let tree = try XCTUnwrap(try fixture()["tree"] as? [String: Any])
        XCTAssertEqual(TreePublicKeys.of(secret: try SealedContentFixtureTests.hex(tree["secret"])), tree["publicKey"] as? String)
        let other = try XCTUnwrap(try fixture()["otherTree"] as? [String: Any])
        XCTAssertEqual(TreePublicKeys.of(secret: try SealedContentFixtureTests.hex(other["secret"])), other["publicKey"] as? String)
    }

    /// wrap-signature.json `resign`: the server's rule, by name.
    func testReplaysTheResignCases() throws {
        let block = try XCTUnwrap(try fixture()["resign"] as? [String: Any], "resign")
        XCTAssertEqual(block["procedure"] as? String, "sessionKeys.resign")
        XCTAssertEqual(block["maxWraps"] as? Int, resignMaxWraps)
        let cases = try XCTUnwrap(block["cases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let name = try XCTUnwrap(c["name"] as? String)
            let got = resignOutcome(
                callerReader: try XCTUnwrap(c["callerReader"] as? Bool),
                currentEpoch: try XCTUnwrap(c["currentEpoch"] as? Bool),
                targetReader: try XCTUnwrap(c["targetReader"] as? Bool),
                signatureValid: try XCTUnwrap(c["signatureValid"] as? Bool),
                row: try XCTUnwrap(ResignRow(rawValue: try XCTUnwrap(c["row"] as? String)), name)
            )
            XCTAssertEqual(got.rule.rawValue, c["rule"] as? String, name)
            XCTAssertEqual(got.outcome.rawValue, c["outcome"] as? String, name)
        }
        let outcomes = try XCTUnwrap(block["outcomes"] as? [String: String])
        for rule in try XCTUnwrap(block["order"] as? [String]) {
            let r = try XCTUnwrap(ResignRule(rawValue: rule), rule)
            let facts = (
                callerReader: r != .callerNotReader, currentEpoch: r != .staleEpoch, targetReader: r != .targetNotReader,
                signatureValid: r != .badSignature, row: r == .noRow ? ResignRow.none : r == .signedRow ? .signed : .unsigned
            )
            XCTAssertEqual(resignOutcome(callerReader: facts.callerReader, currentEpoch: facts.currentEpoch, targetReader: facts.targetReader, signatureValid: facts.signatureValid, row: facts.row).outcome.rawValue, outcomes[rule], rule)
        }
    }

    func testReplaysTheResignFailureCases() throws {
        let block = try XCTUnwrap(try fixture()["resign"] as? [String: Any], "resign")
        let cases = try XCTUnwrap(block["failureCases"] as? [[String: Any]])
        XCTAssertEqual(cases.count, 5)
        for c in cases {
            XCTAssertEqual(resignAfter(code: c["code"] as? String).rawValue, c["after"] as? String, c["name"] as? String ?? "")
        }
    }

    #if ENET_NODE
    struct Row: Equatable {
        var wrapped: String
        var signature: String?
        var signer: String?
        var madeBy: String
    }

    /// `sessionKeys.*` as the server answers and checks them.
    final class VerifyingKeys: SessionKeysRegistry, @unchecked Sendable {
        /// installId → (endpoint id, X25519 public).
        let installs = Locked<[String: (endpointId: String, x25519: String)]>([:])
        /// rootId → readers.
        let readers = Locked<[String: Set<String>]>([:])
        /// rootId → (epoch, public key).
        let roots = Locked<[String: (epoch: UInt32, publicKey: String)]>([:])
        /// `root\nepoch\ninstall` → the stored wrap.
        let rows = Locked<[String: Row]>([:])
        let dueRoots = Locked<[DueTreeKey]>([])
        let refusals = Locked<[String]>([])
        let resignCalls = Locked<[[ResignedTreeKeyWrap]]>([])
        let resignFails = Locked<String?>(nil)

        static func key(_ root: String, _ epoch: UInt32, _ install: String) -> String { "\(root)\n\(epoch)\n\(install)" }

        private func refuse(_ code: String, _ message: String) -> Error {
            refusals.mutate { $0.append(message) }
            return TrpcError.httpError(400, #"{"error":{"json":{"message":"\#(message)","data":{"code":"\#(code)"}}}}"#)
        }

        /// The server's check: the caller's install signed exactly this row.
        private func check(caller: String, root: String, epoch: UInt32, publicKey: String, target: String, wrapped: String, signature: String?) throws -> Row {
            guard let signature else { return Row(wrapped: wrapped, signature: nil, signer: nil, madeBy: caller) }
            let signer = try XCTUnwrap(installs.value[caller]?.endpointId)
            guard let recipient = installs.value[target]?.endpointId,
                  try verifyWrapSignature(
                      statement: EnetWrapStatement(rootId: root, epoch: epoch, treePublicKey: publicKey, recipientEndpointId: recipient, wrapped: wrapped),
                      signature: signature,
                      signerEndpointId: signer
                  )
            else { throw refuse("BAD_REQUEST", "A wrap's signature is not your install's over this key") }
            return Row(wrapped: wrapped, signature: signature, signer: signer, madeBy: caller)
        }

        func sendTarget(accountId: String, agentId: String?, appId: String?) async throws -> ServerSendTarget {
            ServerSendTarget(rootId: nil, epoch: nil, publicKey: nil)
        }

        func create(accountId: String, input: CreateTreeKeyInput) async throws {
            let current = roots.value[input.rootId]?.epoch ?? 0
            guard input.epoch == current + 1 else { throw refuse("CONFLICT", "This chat's encryption key changed. Try again in a moment.") }
            var out: [String: Row] = [:]
            for wrap in input.wraps {
                guard readers.value[input.rootId]?.contains(wrap.installId) == true else { throw refuse("BAD_REQUEST", "not a reader") }
                out[Self.key(input.rootId, input.epoch, wrap.installId)] = try check(
                    caller: input.installId, root: input.rootId, epoch: input.epoch, publicKey: input.publicKey,
                    target: wrap.installId, wrapped: wrap.wrapped, signature: wrap.signature
                )
            }
            rows.mutate { $0.merge(out) { $1 } }
            roots.mutate { $0[input.rootId] = (input.epoch, input.publicKey) }
            dueRoots.mutate { $0.removeAll { $0.rootId == input.rootId } }
        }

        func mine(accountId: String, installId: String, rootIds: [String]?) async throws -> [HeldTreeKey] {
            rows.value.compactMap { key, row in
                let parts = key.split(separator: "\n").map(String.init)
                guard parts[2] == installId, rootIds?.contains(parts[0]) ?? true else { return nil }
                return HeldTreeKey(rootId: parts[0], epoch: UInt32(parts[1])!, wrapped: row.wrapped, signature: row.signature, signerEndpointId: row.signer)
            }
        }

        func pending(accountId: String, installId: String) async throws -> [PendingTreeKey] {
            var out: [PendingTreeKey] = []
            for (root, current) in roots.value where rows.value[Self.key(root, current.epoch, installId)] != nil {
                for reader in (readers.value[root] ?? []).sorted() where rows.value[Self.key(root, current.epoch, reader)] == nil {
                    let install = installs.value[reader]!
                    out.append(PendingTreeKey(
                        rootId: root, epoch: current.epoch, publicKey: current.publicKey,
                        targetInstallId: reader, targetX25519Public: install.x25519, targetEndpointId: install.endpointId
                    ))
                }
            }
            return out
        }

        func wrap(accountId: String, installId: String, wraps: [PendingTreeKeyWrap]) async throws {
            var out: [String: Row] = [:]
            for wrap in wraps {
                guard let current = roots.value[wrap.rootId], current.epoch == wrap.epoch,
                      readers.value[wrap.rootId]?.contains(wrap.targetInstallId) == true
                else { throw refuse("FORBIDDEN", "Not a pending pair") }
                let key = Self.key(wrap.rootId, wrap.epoch, wrap.targetInstallId)
                if rows.value[key] != nil { continue }
                out[key] = try check(
                    caller: installId, root: wrap.rootId, epoch: wrap.epoch, publicKey: current.publicKey,
                    target: wrap.targetInstallId, wrapped: wrap.wrapped, signature: wrap.signature
                )
            }
            rows.mutate { $0.merge(out) { $1 } }
        }

        func publicKeys(accountId: String, rootIds: [String]) async throws -> [TreePublicKey] {
            rootIds.compactMap { root in roots.value[root].map { TreePublicKey(rootId: root, epoch: $0.epoch, publicKey: $0.publicKey) } }
        }

        func requestKey(accountId: String, rootId: String) async throws -> RequestKeyResult { RequestKeyResult(state: "noHost") }

        func due(accountId: String, installId: String) async throws -> [DueTreeKey] { dueRoots.value }

        /// `sessionKeys.resign` by the fixture's rule (`resignOutcome`): a
        /// signature is REQUIRED; an unsigned row of the current epoch is
        /// replaced, a signed one never touched; a refusal writes nothing.
        func resign(accountId: String, installId: String, wraps: [ResignedTreeKeyWrap]) async throws -> ResignResult {
            if let code = resignFails.value { throw refuse(code, "No procedure") }
            resignCalls.mutate { $0.append(wraps) }
            let pairs = wraps.map { Self.key($0.rootId, $0.epoch, $0.installId) }
            guard Set(pairs).count == pairs.count else { throw refuse("BAD_REQUEST", "One wrap per pair") }
            var out: [String: Row] = [:]
            var skipped = 0
            for wrap in wraps {
                let current = roots.value[wrap.rootId]
                let currentEpoch = current?.epoch == wrap.epoch
                var valid = false
                if currentEpoch, let current, let signer = installs.value[installId]?.endpointId, let recipient = installs.value[wrap.installId]?.endpointId {
                    valid = try verifyWrapSignature(
                        statement: EnetWrapStatement(rootId: wrap.rootId, epoch: wrap.epoch, treePublicKey: current.publicKey, recipientEndpointId: recipient, wrapped: wrap.wrapped),
                        signature: wrap.signature, signerEndpointId: signer
                    )
                }
                let key = Self.key(wrap.rootId, wrap.epoch, wrap.installId)
                let stored = rows.value[key]
                let (rule, outcome) = resignOutcome(
                    callerReader: readers.value[wrap.rootId]?.contains(installId) == true,
                    currentEpoch: currentEpoch,
                    targetReader: readers.value[wrap.rootId]?.contains(wrap.installId) == true,
                    signatureValid: valid,
                    row: stored == nil ? .none : stored?.signature == nil ? .unsigned : .signed
                )
                switch outcome {
                case .refused:
                    throw rule == .badSignature
                        ? refuse("BAD_REQUEST", "A wrap's signature is not your install's over this key")
                        : refuse("FORBIDDEN", rule.rawValue)
                case .skipped: skipped += 1
                case .replaced: out[key] = Row(wrapped: wrap.wrapped, signature: wrap.signature, signer: installs.value[installId]?.endpointId, madeBy: installId)
                }
            }
            rows.mutate { $0.merge(out) { $1 } }
            return ResignResult(replaced: out.count, skipped: skipped)
        }
    }

    struct FakeInstalls: TreeReaderDirectory {
        let mineRows: [InstallEntry]
        init(_ mine: [InstallEntry]) { mineRows = mine }
        func mine(accountId: String) async throws -> [InstallEntry] { mineRows }
        func list(accountId: String, teamId: String) async throws -> [InstallEntry] { [] }
    }

    private let crypto = EnetSealedCrypto()
    private let meIdentity = Data((0x40..<0x60).map { UInt8($0) })
    private let peerIdentity = Data((0x60..<0x80).map { UInt8($0) })
    private let root = "01234567-89ab-4def-8123-456789abcdef"

    private func entry(_ id: String, _ identity: Data) throws -> InstallEntry {
        InstallEntry(
            installId: id, userId: "u", kind: "ios",
            endpointId: try endpointIdOf(identity: identity),
            x25519Public: try crypto.installKey(identity: identity).publicKey
        )
    }

    private func server(readers: [String] = ["me", "peer"]) throws -> VerifyingKeys {
        let keys = VerifyingKeys()
        let me = try entry("me", meIdentity), peer = try entry("peer", peerIdentity)
        keys.installs.mutate { $0 = ["me": (me.endpointId, me.x25519Public), "peer": (peer.endpointId, peer.x25519Public)] }
        keys.readers.mutate { $0 = [root: Set(readers)] }
        return keys
    }

    private func holder(_ keys: VerifyingKeys, signer: WrapSigner? = EnetWrapSigner(), ring: SealedKeyRing = SealedKeyRing(persistence: .memory)) throws -> TreeKeyHolder {
        let api = FakeInstalls(([try entry("me", meIdentity), try entry("peer", peerIdentity)]))
        let identity = meIdentity
        return TreeKeyHolder(
            keys: keys, installs: api, crypto: crypto, signer: signer, ring: ring,
            identity: { identity }, installId: { _ in "me" }
        )
    }

    /// Every stored row verifies for its own (root, epoch, stored key, recipient, bytes).
    private func assertEveryRowSignedByMe(_ keys: VerifyingKeys, file: StaticString = #filePath, line: UInt = #line) throws {
        let me = try endpointIdOf(identity: meIdentity)
        for (key, row) in keys.rows.value where row.madeBy == "me" {
            let parts = key.split(separator: "\n").map(String.init)
            let signature = try XCTUnwrap(row.signature, key, file: file, line: line)
            XCTAssertEqual(row.signer, me, file: file, line: line)
            let epoch = UInt32(parts[1])!
            // The key of that epoch: the root's current one is the only epoch these tests write to.
            let publicKey = try XCTUnwrap(keys.roots.value[parts[0]]).publicKey
            XCTAssertEqual(keys.roots.value[parts[0]]?.epoch, epoch, file: file, line: line)
            XCTAssertTrue(try verifyWrapSignature(
                statement: EnetWrapStatement(rootId: parts[0], epoch: epoch, treePublicKey: publicKey,
                                             recipientEndpointId: try XCTUnwrap(keys.installs.value[parts[2]]).endpointId, wrapped: row.wrapped),
                signature: signature, signerEndpointId: me
            ), key, file: file, line: line)
        }
    }

    func testTheSignerSignsTheVectorByteExact() throws {
        let f = try fixture()
        let v = try XCTUnwrap(f["vector"] as? [String: Any])
        let signer = try XCTUnwrap(f["signer"] as? [String: Any])
        let identity = try SealedContentFixtureTests.hex(signer["identity"])
        XCTAssertEqual(try EnetWrapSigner().endpointId(identity: identity), signer["endpointId"] as? String)
        let signature = try EnetWrapSigner().sign(identity: identity, statement: WrapStatement(
            rootId: try XCTUnwrap(v["rootId"] as? String), epoch: 1, treePublicKey: try XCTUnwrap(v["treePublicKey"] as? String),
            recipientEndpointId: try XCTUnwrap(v["recipientEndpointId"] as? String), wrapped: try XCTUnwrap(v["wrapped"] as? String)
        ))
        XCTAssertEqual(signature, v["signature"] as? String)
    }

    func testAFirstKeySignsEveryReadersWrap() async throws {
        let keys = try server()
        let holder = try holder(keys)
        let outcome = await holder.createKey(accountId: "a", rootId: root, teamId: nil, deviceIds: [], shared: false)
        guard case .keyed = outcome else { return XCTFail("\(outcome)") }
        XCTAssertEqual(Set(keys.rows.value.keys), [VerifyingKeys.key(root, 1, "me"), VerifyingKeys.key(root, 1, "peer")])
        XCTAssertTrue(keys.refusals.value.isEmpty, "\(keys.refusals.value)")
        try assertEveryRowSignedByMe(keys)
    }

    func testARotationSignsTheNewEpochsWrap() async throws {
        let keys = try server()
        let ring = SealedKeyRing(persistence: .memory)
        let holder = try holder(keys, ring: ring)
        guard case .keyed = await holder.createKey(accountId: "a", rootId: root, teamId: nil, deviceIds: [], shared: false) else { return XCTFail() }
        keys.dueRoots.mutate { $0 = [DueTreeKey(rootId: root, epoch: 1)] }
        let rotated = await holder.rotateDue(accountId: "a", installId: "me")
        XCTAssertEqual(rotated, 1)
        XCTAssertEqual(keys.roots.value[root]?.epoch, 2)
        let row = try XCTUnwrap(keys.rows.value[VerifyingKeys.key(root, 2, "me")])
        XCTAssertNotNil(row.signature)
        XCTAssertTrue(keys.refusals.value.isEmpty, "\(keys.refusals.value)")
        // Epoch 2's rows verify over epoch 2's key; epoch 1's stay as they were.
        keys.rows.mutate { $0 = $0.filter { $0.key.contains("\n2\n") } }
        try assertEveryRowSignedByMe(keys)
    }

    func testPendingAnswersAreSignedOverTheHeldKey() async throws {
        let keys = try server()
        // The tree was keyed with our own wrap only (by an older build: unsigned).
        let tree = crypto.generateTreeKey()
        let me = try entry("me", meIdentity)
        keys.roots.mutate { $0[root] = (1, tree.publicKey) }
        keys.rows.mutate { $0[VerifyingKeys.key(root, 1, "me")] = Row(wrapped: try! self.crypto.wrap(installPublic: me.x25519Public, treeSecret: tree.secret), signature: nil, signer: nil, madeBy: "old") }
        let holder = try holder(keys)
        keys.resignFails.mutate { $0 = "NOT_FOUND" }
        await holder.sync(accountId: "a")
        let peer = try XCTUnwrap(keys.rows.value[VerifyingKeys.key(root, 1, "peer")])
        XCTAssertEqual(peer.madeBy, "me")
        XCTAssertNotNil(peer.signature)
        XCTAssertTrue(keys.refusals.value.allSatisfy { $0 == "No procedure" }, "\(keys.refusals.value)")
        try assertEveryRowSignedByMe(keys)
    }

    /// A pair naming a key that is not the held secret's goes unsigned: the
    /// signature names only the key this install holds.
    func testAPendingPairNamingAnotherKeyGoesUnsigned() async throws {
        let keys = try server()
        let tree = crypto.generateTreeKey()
        let me = try entry("me", meIdentity)
        keys.roots.mutate { $0[root] = (1, crypto.generateTreeKey().publicKey) }
        keys.rows.mutate { $0[VerifyingKeys.key(root, 1, "me")] = Row(wrapped: try! self.crypto.wrap(installPublic: me.x25519Public, treeSecret: tree.secret), signature: nil, signer: nil, madeBy: "old") }
        keys.resignFails.mutate { $0 = "NOT_FOUND" }
        try await holder(keys).sync(accountId: "a")
        let peer = try XCTUnwrap(keys.rows.value[VerifyingKeys.key(root, 1, "peer")])
        XCTAssertNil(peer.signature)
        XCTAssertFalse(keys.refusals.value.contains { $0.contains("signature") })
    }

    /// Decision (c): a key held from before is signed again: our own wrap
    /// and the account's other installs, new bytes made here, once per root.
    func testAKeyHeldFromBeforeIsSignedAgainOnce() async throws {
        let keys = try server()
        let tree = crypto.generateTreeKey()
        let me = try entry("me", meIdentity), peer = try entry("peer", peerIdentity)
        keys.roots.mutate { $0[root] = (1, tree.publicKey) }
        let old = [
            "me": try crypto.wrap(installPublic: me.x25519Public, treeSecret: tree.secret),
            "peer": try crypto.wrap(installPublic: peer.x25519Public, treeSecret: tree.secret),
        ]
        keys.rows.mutate { rows in for (install, wrapped) in old { rows[VerifyingKeys.key(root, 1, install)] = Row(wrapped: wrapped, signature: nil, signer: nil, madeBy: "old") } }
        let holder = try holder(keys)
        await holder.sync(accountId: "a")
        XCTAssertEqual(keys.resignCalls.value.count, 1)
        let sent = try XCTUnwrap(keys.resignCalls.value.first)
        XCTAssertEqual(sent.map(\.installId), ["me", "peer"])
        // Never the old bytes: a holder signs only wraps it made.
        XCTAssertTrue(Set(sent.map(\.wrapped)).isDisjoint(with: Set(old.values)))
        XCTAssertTrue(keys.rows.value.values.allSatisfy { $0.madeBy == "me" })
        XCTAssertTrue(keys.refusals.value.isEmpty, "\(keys.refusals.value)")
        try assertEveryRowSignedByMe(keys)
        await holder.sync(accountId: "a")
        XCTAssertEqual(keys.resignCalls.value.count, 1)
    }

    /// An older server has no `resign`: not yet, asked again on a later pass.
    func testAnOlderServerIsAskedAgainLater() async throws {
        let keys = try server()
        let tree = crypto.generateTreeKey()
        let me = try entry("me", meIdentity)
        keys.roots.mutate { $0[root] = (1, tree.publicKey) }
        keys.rows.mutate { $0[VerifyingKeys.key(root, 1, "me")] = Row(wrapped: try! self.crypto.wrap(installPublic: me.x25519Public, treeSecret: tree.secret), signature: nil, signer: nil, madeBy: "old") }
        keys.resignFails.mutate { $0 = "NOT_FOUND" }
        let holder = try holder(keys)
        await holder.sync(accountId: "a")
        XCTAssertNil(keys.rows.value[VerifyingKeys.key(root, 1, "me")]?.signature)
        keys.resignFails.mutate { $0 = nil }
        await holder.sync(accountId: "a")
        XCTAssertNotNil(keys.rows.value[VerifyingKeys.key(root, 1, "me")]?.signature)
        try assertEveryRowSignedByMe(keys)
    }

    /// A blip (`retry`) keeps the root for the next pass; the pass goes on.
    func testABlipIsRetriedOnTheNextPass() async throws {
        let keys = try server()
        let tree = crypto.generateTreeKey()
        let me = try entry("me", meIdentity)
        keys.roots.mutate { $0[root] = (1, tree.publicKey) }
        keys.rows.mutate { $0[VerifyingKeys.key(root, 1, "me")] = Row(wrapped: try! self.crypto.wrap(installPublic: me.x25519Public, treeSecret: tree.secret), signature: nil, signer: nil, madeBy: "old") }
        keys.resignFails.mutate { $0 = "INTERNAL_SERVER_ERROR" }
        let holder = try holder(keys)
        await holder.sync(accountId: "a")
        keys.resignFails.mutate { $0 = nil }
        await holder.sync(accountId: "a")
        XCTAssertNotNil(keys.rows.value[VerifyingKeys.key(root, 1, "me")]?.signature)
        try assertEveryRowSignedByMe(keys)
    }

    /// A target the server does not count as a reader refuses the call: our
    /// own wrap is signed again alone.
    func testANonReaderFallsBackToTheOwnWrap() async throws {
        let keys = try server(readers: ["me"])
        let tree = crypto.generateTreeKey()
        let me = try entry("me", meIdentity)
        keys.roots.mutate { $0[root] = (1, tree.publicKey) }
        keys.rows.mutate { $0[VerifyingKeys.key(root, 1, "me")] = Row(wrapped: try! self.crypto.wrap(installPublic: me.x25519Public, treeSecret: tree.secret), signature: nil, signer: nil, madeBy: "old") }
        try await holder(keys).sync(accountId: "a")
        XCTAssertEqual(keys.resignCalls.value.map { $0.map(\.installId) }, [["me", "peer"], ["me"]])
        try assertEveryRowSignedByMe(keys)
    }

    /// A new root's inline seal cannot be signed (its id is unknown): it goes
    /// unsigned, and once adopted the same readers' wraps are signed again.
    func testANewRootsInlineSealIsSignedAgainOnceItExists() async throws {
        let keys = try server()
        let holder = try holder(keys)
        let (seal, key) = try await holder.newChatSeal(accountId: "a", teamId: "t", deviceIds: [], shared: false)
        XCTAssertEqual(seal.wraps.map(\.installId), ["me", "peer"])
        XCTAssertTrue(seal.wraps.allSatisfy { $0.signature == nil })
        // The server creates the root from the seal (`sessions.send`).
        try await keys.create(accountId: "a", input: CreateTreeKeyInput(rootId: root, installId: seal.installId, epoch: 1, publicKey: seal.publicKey, wraps: seal.wraps))
        await holder.adopt(rootId: root, key: key)
        await holder.sync(accountId: "a")
        XCTAssertEqual(keys.resignCalls.value.map { $0.map(\.installId) }, [["me", "peer"]])
        XCTAssertTrue(keys.refusals.value.isEmpty, "\(keys.refusals.value)")
        try assertEveryRowSignedByMe(keys)
    }

    /// Signing again is bounded per pass.
    func testSigningAgainIsBoundedPerPass() async throws {
        let keys = try server()
        let me = try entry("me", meIdentity)
        let count = TreeKeyHolder.resignBatch + 5
        let ids = (0..<count).map { String(format: "01234567-89ab-4def-8123-%012d", $0) }
        for id in ids {
            let tree = crypto.generateTreeKey()
            keys.readers.mutate { $0[id] = ["me", "peer"] }
            keys.roots.mutate { $0[id] = (1, tree.publicKey) }
            keys.rows.mutate { $0[VerifyingKeys.key(id, 1, "me")] = Row(wrapped: try! self.crypto.wrap(installPublic: me.x25519Public, treeSecret: tree.secret), signature: nil, signer: nil, madeBy: "old") }
        }
        let holder = try holder(keys)
        await holder.sync(accountId: "a")
        XCTAssertEqual(keys.resignCalls.value.count, TreeKeyHolder.resignBatch)
        await holder.sync(accountId: "a")
        XCTAssertEqual(keys.resignCalls.value.count, count)
        try assertEveryRowSignedByMe(keys)
    }

    /// No signer (the push-only binding of the extensions): every wrap unsigned, nothing signed again.
    func testWithoutASignerNothingIsSigned() async throws {
        let keys = try server()
        let holder = try holder(keys, signer: nil)
        guard case .keyed = await holder.createKey(accountId: "a", rootId: root, teamId: nil, deviceIds: [], shared: false) else { return XCTFail() }
        await holder.sync(accountId: "a")
        XCTAssertTrue(keys.rows.value.values.allSatisfy { $0.signature == nil })
        XCTAssertTrue(keys.resignCalls.value.isEmpty)
    }
    #else
    func testTheBindingIsBuilt() throws {
        throw XCTSkip("the full ExponentialNetFFI.xcframework was not built (build-ios.sh)")
    }
    #endif
}
