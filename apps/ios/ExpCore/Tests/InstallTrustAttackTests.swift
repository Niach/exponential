@testable import ExpCore
import Foundation
import XCTest

// #EXP-1950 THE MALICIOUS SERVER (install-trust.json `maliciousServer`, the
// threat model's four attacks): ONE fake API that lies in `sessionKeys.mine`,
// `pending`, `publicKeys`, `sessions.sendTarget` and `installs.trust`,
// answering EXACTLY each case's `answers`, against the REAL holder
// (`TreeKeyHolder` + `InstallTrustKeeper`, the full eNet binding) of the
// fixture's victim install A. Every case runs in `record` mode (release 1:
// its `now`) and `mandatory` mode (release 2: its `mandatory`). Then the
// negatives: never a wrap for an install that asked and is not pinned,
// never an approval by itself, never a row whose code the server supplied.
#if ENET_NODE
final class InstallTrustAttackTests: XCTestCase {
    typealias Object = [String: Any]

    /// The lying server: every procedure the holder asks answers what the
    /// case says; everything the holder writes is recorded.
    final class LyingServer: SessionKeysRegistry, TreeReaderDirectory, InstallTrustApi, @unchecked Sendable {
        let mineRows = Locked<[HeldTreeKey]>([])
        let pendingRows = Locked<[PendingTreeKey]>([])
        let publicKeyRows = Locked<[TreePublicKey]>([])
        let target = Locked(ServerSendTarget(rootId: nil, epoch: nil, publicKey: nil))
        let trustAnswer = Locked(TrustAnswer(installs: [], approvals: []))
        let ownInstalls = Locked<[InstallEntry]>([])
        /// What the holder did.
        let wrapped = Locked<[PendingTreeKeyWrap]>([])
        let created = Locked<[CreateTreeKeyInput]>([])
        let approved = Locked<[(kind: ApprovalKind, subject: String)]>([])
        let revoked = Locked<[String]>([])
        let requested = Locked(0)
        let claimed = Locked(0)
        let claimApprovals = Locked<[RootClaimApproval]>([])

        func sendTarget(accountId: String, agentId: String?, appId: String?) async throws -> ServerSendTarget { target.value }
        func create(accountId: String, input: CreateTreeKeyInput) async throws { created.mutate { $0.append(input) } }
        func mine(accountId: String, installId: String, rootIds: [String]?) async throws -> [HeldTreeKey] {
            mineRows.value.filter { rootIds?.contains($0.rootId.lowercased()) ?? true }
        }
        func pending(accountId: String, installId: String) async throws -> [PendingTreeKey] { pendingRows.value }
        func wrap(accountId: String, installId: String, wraps: [PendingTreeKeyWrap]) async throws { wrapped.mutate { $0 += wraps } }
        func publicKeys(accountId: String, rootIds: [String]) async throws -> [TreePublicKey] {
            publicKeyRows.value.filter { rootIds.contains($0.rootId) }
        }
        func requestKey(accountId: String, rootId: String) async throws -> RequestKeyResult { RequestKeyResult(state: "noHost") }

        func mine(accountId: String) async throws -> [InstallEntry] { ownInstalls.value }
        func list(accountId: String, teamId: String) async throws -> [InstallEntry] { [] }

        func trust(accountId: String, installId: String) async throws -> TrustAnswer { trustAnswer.value }
        func requestApproval(accountId: String, installId: String) async throws { requested.mutate { $0 += 1 } }
        func claimRoot(accountId: String, installId: String, signature: String, approvals: [RootClaimApproval]?) async throws {
            claimed.mutate { $0 += 1 }
            claimApprovals.mutate { $0 += approvals ?? [] }
        }
        func approve(accountId: String, installId: String, subjectInstallId: String, kind: ApprovalKind, signature: String) async throws {
            approved.mutate { $0.append((kind, subjectInstallId)) }
        }
        func revoke(accountId: String, installId: String) async throws { revoked.mutate { $0.append(installId) } }
    }

    /// An account's installs as the server stores them (web's
    /// `accountServer`), honest unless told to lie: `claimRoot` stores the
    /// root and its approvals in one go (CONFLICT while another install is
    /// on a chain), `approve` stores one.
    final class AccountServer: InstallTrustApi, @unchecked Sendable {
        let installs: Locked<[ServedInstall]>
        let approvals = Locked<[ServedApproval]>([])
        let asked = Locked<[String]>([])
        let claims = Locked<[String]>([])
        /// Holds every claim until `claimGate` claims arrived (two installs at once).
        var claimGate = 0
        private let gate = Locked<(arrived: Int, waiters: [CheckedContinuation<Void, Never>])>((0, []))

        init(_ installs: [ServedInstall]) { self.installs = Locked(installs) }

        private func endpoint(of installId: String) throws -> String {
            guard let row = installs.value.first(where: { $0.installId == installId }) else { throw URLError(.badServerResponse) }
            return row.endpointId
        }

        func trust(accountId: String, installId: String) async throws -> TrustAnswer {
            let asked = asked.value
            return TrustAnswer(
                installs: installs.value.map { row in
                    var row = row
                    row.approvalRequested = row.approvalRequested || asked.contains(row.installId)
                    return row
                },
                approvals: approvals.value
            )
        }

        func requestApproval(accountId: String, installId: String) async throws { asked.mutate { $0.append(installId) } }

        func claimRoot(accountId: String, installId: String, signature: String, approvals claimed: [RootClaimApproval]?) async throws {
            claims.mutate { $0.append(installId) }
            if claimGate > 0 {
                let needed = claimGate
                await withCheckedContinuation { (continuation: CheckedContinuation<Void, Never>) in
                    gate.mutate { state in
                        state.arrived += 1
                        state.waiters.append(continuation)
                        guard state.arrived >= needed else { return }
                        for waiter in state.waiters { waiter.resume() }
                        state.waiters = []
                    }
                }
            }
            let me = try endpoint(of: installId)
            let approves = try (claimed ?? []).map {
                ServedApproval(kind: .approve, subjectEndpointId: try endpoint(of: $0.subjectInstallId), approverEndpointId: me, signature: $0.signature)
            }
            var conflict = false
            approvals.mutate { stored in
                if stored.contains(where: { ($0.kind == .root || $0.kind == .approve) && $0.subjectEndpointId != me }) {
                    conflict = true
                    return
                }
                stored.append(ServedApproval(kind: .root, subjectEndpointId: me, approverEndpointId: me, signature: signature))
                stored += approves
            }
            if conflict { throw TrpcError.httpError(409, #"{"error":{"json":{"message":"CONFLICT","data":{"code":"CONFLICT"}}}}"#) }
        }

        func approve(accountId: String, installId: String, subjectInstallId: String, kind: ApprovalKind, signature: String) async throws {
            let subject = try endpoint(of: subjectInstallId), approver = try endpoint(of: installId)
            approvals.mutate { $0.append(ServedApproval(kind: kind, subjectEndpointId: subject, approverEndpointId: approver, signature: signature)) }
        }

        func revoke(accountId: String, installId: String) async throws {}
    }

    private let account = "acct"
    private let sealed = EnetSealedCrypto()

    private struct Victim {
        let server: LyingServer
        let keeper: InstallTrustKeeper
        let holder: TreeKeyHolder
        let ring: SealedKeyRing
    }

    /// The fixture's victim A (joined; pins A, B, T1; verified epoch 1 of
    /// 0123… and 2 of 2123…), its keyring holding the real secrets, behind
    /// a server whose `installs.trust` serves A, B (approved by A) and T1
    /// (a team-mate's root) truthfully, plus whatever the case adds.
    private func victim(_ fx: TrustFixture, mode: TrustMode, extra: [ServedInstall] = [], roots: [String] = []) async throws -> Victim {
        let server = LyingServer()
        let keys = fx.keys
        let a = try fx.served(["key": "A"]), b = try fx.served(["key": "B"]), t1 = try fx.served(["key": "T1"])
        var approvals = [
            ServedApproval(kind: .root, subjectEndpointId: a.endpointId, approverEndpointId: a.endpointId,
                           signature: try InstallApproval.sign(.root, subject: a, identity: try keys.identity("A"), crypto: fx.crypto)),
            ServedApproval(kind: .approve, subjectEndpointId: b.endpointId, approverEndpointId: a.endpointId,
                           signature: try InstallApproval.sign(.approve, subject: b, identity: try keys.identity("A"), crypto: fx.crypto)),
            ServedApproval(kind: .root, subjectEndpointId: t1.endpointId, approverEndpointId: t1.endpointId,
                           signature: try InstallApproval.sign(.root, subject: t1, identity: try keys.identity("T1"), crypto: fx.crypto)),
        ]
        for name in roots {
            let subject = try XCTUnwrap(extra.first { $0.endpointId == (try? keys.endpoint(name)) }, name)
            approvals.append(ServedApproval(kind: .root, subjectEndpointId: subject.endpointId, approverEndpointId: subject.endpointId,
                                            signature: try InstallApproval.sign(.root, subject: subject, identity: try keys.identity(name), crypto: fx.crypto)))
        }
        server.trustAnswer.mutate { $0 = TrustAnswer(installs: [a, b, t1] + extra, approvals: approvals) }
        let persistence = TrustStorePersistence.memory()
        persistence.save(account, try JSONEncoder().encode(try fx.namedStore("victim").store))
        let identity = try keys.identity("A")
        let keeper = InstallTrustKeeper(api: server, crypto: fx.crypto, persistence: persistence, identity: { identity }, mode: mode)
        let ring = SealedKeyRing(persistence: .memory)
        // The real keys the victim holds (callCases' honest secret is epoch 1 of both roots).
        let real = Data((0x20..<0x40).map { UInt8($0) })
        ring.add([("01234567-89ab-4def-8123-456789abcdef", 1, real), ("21234567-89ab-4def-8123-456789abcdef", 1, real)])
        let holder = TreeKeyHolder(
            keys: server, installs: server, crypto: sealed, trust: keeper, ring: ring,
            identity: { identity }, installId: { _ in "install-A" }
        )
        return Victim(server: server, keeper: keeper, holder: holder, ring: ring)
    }

    private func attackCases() throws -> [(attack: Int, c: Object)] {
        let f = try InstallTrustFixtureTests.fixture()
        return try InstallTrustFixtureTests.cases(f, "maliciousServer").flatMap { entry in
            try InstallTrustFixtureTests.cases(entry, "cases").map { (try XCTUnwrap(entry["attack"] as? Int), $0) }
        }
    }

    private func source(_ c: Object) throws -> Object {
        let rules = try InstallTrustFixtureTests.object(try InstallTrustFixtureTests.fixture(), "rules")
        let name = try InstallTrustFixtureTests.text(c, "name")
        return try XCTUnwrap(try InstallTrustFixtureTests.cases(rules, try InstallTrustFixtureTests.text(c, "replay")).first { $0["name"] as? String == name }, name)
    }

    private func decode<T: Decodable>(_ type: T.Type, _ value: Any?) throws -> T {
        try JSONDecoder().decode(T.self, from: try JSONSerialization.data(withJSONObject: try XCTUnwrap(value), options: [.fragmentsAllowed]))
    }

    private func want(_ c: Object, _ mode: TrustMode) throws -> Object {
        try InstallTrustFixtureTests.object(c, mode == .record ? "now" : "mandatory")
    }

    /// A held secret a seal / call case starts from, and its record.
    private func seedHeld(_ v: Victim, _ source: Object, rootId: String, epoch: UInt32) async throws {
        guard let held = source["held"] as? Object else { return }
        let secret = try SealedContentFixtureTests.hex(held["secret"])
        v.ring.add([(rootId, epoch, secret)])
        await v.keeper.record(accountId: account, rootId: rootId, epoch: epoch, record: try XCTUnwrap(KeyRecord(rawValue: try InstallTrustFixtureTests.text(held, "record"))))
    }

    // MARK: the four attacks, every case, now and once mandatory

    func testEveryAttackAgainstTheRealHolder() async throws {
        let fx = try TrustFixture()
        var ran: [Int: Int] = [:]
        for (attack, c) in try attackCases() {
            let name = try InstallTrustFixtureTests.text(c, "name")
            let answers = try InstallTrustFixtureTests.object(c, "answers")
            let rule = try InstallTrustFixtureTests.text(c, "replay")
            let src = try source(c)
            for mode in TrustMode.allCases {
                let label = "\(name) (\(mode))"
                let expected = try want(c, mode)
                switch rule {
                case "keepCases":
                    // Attack 1: `sessionKeys.mine` plants a wrap of a key the server made.
                    let v = try await victim(fx, mode: mode)
                    let planted = try decode([HeldTreeKey].self, answers["sessionKeys.mine"])
                    v.server.mineRows.mutate { $0 = planted }
                    await v.holder.sync(accountId: account)
                    let wrap = try XCTUnwrap(planted.first)
                    let kept = v.ring.holds(rootId: wrap.rootId, epoch: wrap.epoch)
                    XCTAssertEqual(kept, expected["keep"] as? Bool, label)
                    let last = await v.keeper.decisions.last { $0.rule == .r1 }
                    if kept {
                        let record = await v.keeper.recordOf(accountId: account, rootId: wrap.rootId, epoch: wrap.epoch)
                        XCTAssertEqual(record.rawValue, expected["record"] as? String, label)
                    } else {
                        XCTAssertEqual(last?.verdict, "refused:\(try InstallTrustFixtureTests.text(expected, "reason"))", label)
                    }
                case "sealCases":
                    // Attacks 1 + 2: `sendTarget` / `publicKeys` name a false (root, epoch, key).
                    let v = try await victim(fx, mode: mode)
                    let key: TreeSealKey, rootId: String
                    if let target = answers["sessions.sendTarget"] {
                        v.server.target.mutate { $0 = (try? self.decode(ServerSendTarget.self, target)) ?? $0 }
                        let answered = await v.holder.sendTarget(accountId: account, agentId: "agent", appId: nil)
                        let served = try XCTUnwrap(answered, label)
                        rootId = try XCTUnwrap(served.rootId)
                        key = try XCTUnwrap(served.key)
                    } else {
                        let rows = try decode([TreePublicKey].self, answers["sessionKeys.publicKeys"])
                        v.server.publicKeyRows.mutate { $0 = rows }
                        rootId = try XCTUnwrap(rows.first?.rootId)
                        let named = await v.holder.serverKey(accountId: account, rootId: rootId)
                        key = try XCTUnwrap(named, label)
                    }
                    try await seedHeld(v, src, rootId: rootId, epoch: key.epoch)
                    let sealedTo = await v.holder.sealAllowed(accountId: account, rootId: rootId, key: key)
                    XCTAssertEqual(sealedTo, expected["seal"] as? Bool, label)
                    let decided = await v.keeper.decisions.last { $0.rule == .r3 }
                    let verdict = try XCTUnwrap(decided, label).verdict
                    let wanted = sealedTo ? try InstallTrustFixtureTests.text(expected, "record") : "refused:\(try InstallTrustFixtureTests.text(expected, "reason"))"
                    XCTAssertEqual(verdict, wanted, label)
                case "targetCases":
                    // Attack 3: `pending` + the install list name the server's reader.
                    let listed = try decode([ServedInstall].self, answers["installs.list"])
                    let roots = try XCTUnwrap(src["roots"] as? [String])
                    // B is served truthfully by `installs.trust` (only `pending` lies about its key).
                    let extra = listed.filter { $0.installId != "install-B" }
                    let v = try await victim(fx, mode: mode, extra: extra, roots: roots)
                    let pairs = try decode([PendingTreeKey].self, answers["sessionKeys.pending"])
                    v.server.pendingRows.mutate { $0 = pairs }
                    await v.holder.sync(accountId: account)
                    let target = try XCTUnwrap(pairs.first?.targetInstallId)
                    let didWrap = v.server.wrapped.value.contains { $0.targetInstallId == target }
                    XCTAssertEqual(didWrap, expected["wrap"] as? Bool, label)
                    let decided = await v.keeper.decisions.last { $0.rule == .r4 && $0.subject == target }
                    let verdict = try XCTUnwrap(decided, label).verdict
                    let wanted = didWrap ? try InstallTrustFixtureTests.text(expected, "record") : "refused:\(try InstallTrustFixtureTests.text(expected, "reason"))"
                    XCTAssertEqual(verdict, wanted, label)
                case "callCases":
                    // Attack 4: a false `sendTarget` puts the server into a sealed call.
                    let v = try await victim(fx, mode: mode)
                    v.server.target.mutate { $0 = (try? self.decode(ServerSendTarget.self, answers["sessions.sendTarget"])) ?? $0 }
                    let keys = HolderCallKeys(holder: v.holder, accountId: account, sealable: false)
                    let answered = await keys.target(agentId: "agent")
                    let served = try XCTUnwrap(answered, label)
                    let rootId = try XCTUnwrap(served.rootId), epoch = try XCTUnwrap(served.epoch)
                    try await seedHeld(v, src, rootId: rootId, epoch: epoch)
                    let secret = await keys.secret(rootId: rootId, epoch: epoch)
                    XCTAssertEqual(secret != nil, expected["key"] as? Bool, label)
                    if let noKey = expected["noKey"] as? String {
                        let decided = await v.keeper.decisions.last { $0.rule == .call }
                        XCTAssertEqual(decided?.verdict, noKey, label)
                    }
                default:
                    XCTFail("\(name): unknown rule \(rule)")
                }
            }
            ran[attack, default: 0] += 1
        }
        XCTAssertEqual(ran.keys.sorted(), [1, 2, 3, 4])
        XCTAssertEqual(ran.values.reduce(0, +), try attackCases().count)
    }

    // MARK: the negatives

    /// A new key's readers and a resign never include an install that asked
    /// and is not approved (the waiting state), whatever the server lists.
    func testNeverWrapsANewKeyForAnUnpinnedAskingInstall() async throws {
        let fx = try TrustFixture()
        var asking = try fx.served(["key": "Z", "approvalRequested": true])
        asking.installId = "install-Z"
        let v = try await victim(fx, mode: .record, extra: [asking])
        await v.holder.sync(accountId: account)
        v.server.ownInstalls.mutate {
            $0 = [
                InstallEntry(installId: "install-A", userId: "user-me", kind: "ios", endpointId: try! fx.keys.endpoint("A"), x25519Public: try! fx.keys.text("A", "x25519Public")),
                InstallEntry(installId: "install-B", userId: "user-me", kind: "web", endpointId: try! fx.keys.endpoint("B"), x25519Public: try! fx.keys.text("B", "x25519Public")),
                InstallEntry(installId: "install-Z", userId: "user-me", kind: "web", endpointId: asking.endpointId, x25519Public: asking.x25519Public),
            ]
        }
        let seal = try await v.holder.newChatSeal(accountId: account, teamId: nil, deviceIds: [], shared: false)
        XCTAssertEqual(Set(seal.seal.wraps.map(\.installId)), ["install-A", "install-B"])
        let created = await v.holder.createKey(accountId: account, rootId: "41234567-89ab-4def-8123-456789abcdef", teamId: nil, deviceIds: [], shared: false)
        guard case .keyed = created else { return XCTFail("\(created)") }
        XCTAssertEqual(Set(try XCTUnwrap(v.server.created.value.last).wraps.map(\.installId)), ["install-A", "install-B"])
        // Its own new key is recorded verified (it made it).
        let record = await v.keeper.recordOf(accountId: account, rootId: "41234567-89ab-4def-8123-456789abcdef", epoch: 1)
        XCTAssertEqual(record, .verified)
    }

    /// A waiting install wraps for nobody but itself.
    func testAWaitingInstallWrapsForNobodyElse() async throws {
        let fx = try TrustFixture()
        let server = LyingServer()
        let a = try fx.served(["key": "A"]), n = try fx.served(["key": "N", "approvalRequested": true])
        server.trustAnswer.mutate {
            $0 = TrustAnswer(installs: [a, n], approvals: [ServedApproval(
                kind: .root, subjectEndpointId: a.endpointId, approverEndpointId: a.endpointId,
                signature: try! InstallApproval.sign(.root, subject: a, identity: try! fx.keys.identity("A"), crypto: fx.crypto)
            )])
        }
        let identity = try fx.keys.identity("N")
        let keeper = InstallTrustKeeper(api: server, crypto: fx.crypto, persistence: .memory(), identity: { identity })
        let ring = SealedKeyRing(persistence: .memory)
        let real = Data((0x20..<0x40).map { UInt8($0) })
        ring.add([("01234567-89ab-4def-8123-456789abcdef", 1, real)])
        let holder = TreeKeyHolder(keys: server, installs: server, crypto: sealed, trust: keeper, ring: ring, identity: { identity }, installId: { _ in "install-N" })
        server.pendingRows.mutate {
            $0 = [PendingTreeKey(
                rootId: "01234567-89ab-4def-8123-456789abcdef", epoch: 1, publicKey: TreePublicKeys.of(secret: real)!,
                targetInstallId: "install-A", targetX25519Public: a.x25519Public, targetEndpointId: a.endpointId
            )]
        }
        await holder.sync(accountId: account)
        let snapshot = await keeper.snapshot(accountId: account)
        XCTAssertEqual(snapshot.state, .waiting)
        XCTAssertEqual(snapshot.code, try fx.keys.text("N", "code"))
        XCTAssertTrue(snapshot.rows.isEmpty)
        XCTAssertTrue(server.wrapped.value.isEmpty)
        // It asked once (it had asked already: `approvalRequested`), never claimed a root.
        XCTAssertEqual(server.requested.value, 0)
        XCTAssertEqual(server.claimed.value, 0)
        let decided = await keeper.decisions.last { $0.rule == .r4 }
        XCTAssertEqual(decided?.verdict, "refused:notTrusted")
    }

    /// A trusted install shows the asking install as a row and NEVER
    /// approves it by itself, over any number of passes; Approve signs and
    /// posts; Remove revokes; a Reset's Remove declines.
    func testNeverApprovesByItselfAndApprovesOnlyWhenPressed() async throws {
        let fx = try TrustFixture()
        var asking = try fx.served(["key": "N", "approvalRequested": true])
        asking.label = "Pixel 9"
        let t3 = try fx.served(["key": "T3"])
        let v = try await victim(fx, mode: .record, extra: [asking, t3], roots: ["T3"])
        for _ in 0..<3 { await v.holder.sync(accountId: account) }
        XCTAssertTrue(v.server.approved.value.isEmpty)
        XCTAssertEqual(v.server.requested.value, 0)
        let snapshot = await v.keeper.snapshot(accountId: account)
        XCTAssertEqual(snapshot.state, .trusted)
        XCTAssertEqual(snapshot.rows.map(\.kind), [.newInstall, .reset])
        XCTAssertEqual(snapshot.rows.map(\.installId), ["install-N", "install-T3"])
        try await v.keeper.approve(accountId: account, row: snapshot.rows[0])
        XCTAssertEqual(v.server.approved.value.map(\.kind), [.approve])
        XCTAssertEqual(v.server.approved.value.map(\.subject), ["install-N"])
        try await v.keeper.remove(accountId: account, row: snapshot.rows[0])
        XCTAssertEqual(v.server.revoked.value, ["install-N"])
        try await v.keeper.remove(accountId: account, row: snapshot.rows[1])
        XCTAssertEqual(v.server.approved.value.map(\.kind), [.approve, .decline])
    }

    /// The code of a row is derived from its endpoint id HERE: a server that
    /// sends its own `code` (or label tricks) never puts its code on a row,
    /// and a row it names whose registration does not verify is never shown.
    func testNeverShowsARowWhoseCodeTheServerSupplied() async throws {
        let fx = try TrustFixture()
        var json = try XCTUnwrap(try JSONSerialization.jsonObject(with: try JSONEncoder().encode(try fx.served(["key": "N", "approvalRequested": true]))) as? Object)
        json["code"] = "AAAAA AAAAA"
        let lying = try decode(ServedInstall.self, json)
        let forged = try fx.served(["key": "L", "approvalRequested": true, "registration": "forged"])
        let v = try await victim(fx, mode: .record, extra: [lying, forged])
        await v.holder.sync(accountId: account)
        let rows = await v.keeper.snapshot(accountId: account).rows
        XCTAssertEqual(rows.map(\.installId), ["install-N"])
        XCTAssertEqual(rows.first?.code, try fx.keys.text("N", "code"))
        XCTAssertEqual(rows.first?.code, InstallCode.of(endpointId: lying.endpointId))
        // `installs.waiting` (the server alone naming rows) is never asked.
        XCTAssertTrue(v.server.approved.value.isEmpty)
    }

    /// An older server without `installs.trust`: everything as before (unavailable, legacy).
    func testAnOlderServerChangesNothing() async throws {
        final class OldServer: InstallTrustApi, @unchecked Sendable {
            func trust(accountId: String, installId: String) async throws -> TrustAnswer {
                throw TrpcError.httpError(404, #"{"error":{"json":{"message":"No procedure","data":{"code":"NOT_FOUND"}}}}"#)
            }
            func requestApproval(accountId: String, installId: String) async throws {}
            func claimRoot(accountId: String, installId: String, signature: String, approvals: [RootClaimApproval]?) async throws {}
            func approve(accountId: String, installId: String, subjectInstallId: String, kind: ApprovalKind, signature: String) async throws {}
            func revoke(accountId: String, installId: String) async throws {}
        }
        let fx = try TrustFixture()
        let identity = try fx.keys.identity("A")
        let keeper = InstallTrustKeeper(api: OldServer(), crypto: fx.crypto, persistence: .memory(), identity: { identity })
        await keeper.sync(accountId: account, installId: "install-A")
        let state = await keeper.snapshot(accountId: account).state
        XCTAssertEqual(state, .unavailable)
        let verdict = await keeper.target(accountId: account, installId: "install-Z", x25519Public: try fx.keys.text("Z", "x25519Public"), endpointId: nil, userId: nil)
        XCTAssertEqual(verdict, .wrap(record: .legacy))
    }

    /// Attack 3's gap, now only under a TEAM-MATE's account: a silent install
    /// there looks like an older build: wrapped and recorded `legacy` now,
    /// refused once mandatory.
    func testASilentTeamMatesInstallIsTheLegacyGap() async throws {
        let fx = try TrustFixture()
        let t2 = try fx.served(["key": "T2"])
        for mode in TrustMode.allCases {
            let v = try await victim(fx, mode: mode, extra: [t2])
            v.server.pendingRows.mutate {
                $0 = [PendingTreeKey(
                    rootId: "01234567-89ab-4def-8123-456789abcdef", epoch: 1, publicKey: TreePublicKeys.of(secret: Data((0x20..<0x40).map { UInt8($0) }))!,
                    targetInstallId: "install-T2", targetX25519Public: t2.x25519Public, targetEndpointId: t2.endpointId
                )]
            }
            await v.holder.sync(accountId: account)
            XCTAssertEqual(v.server.wrapped.value.map(\.targetInstallId), mode == .record ? ["install-T2"] : [], "\(mode)")
            let decided = await v.keeper.decisions.last { $0.rule == .r4 && $0.subject == "install-T2" }
            XCTAssertEqual(decided?.verdict, mode == .record ? "legacy" : "refused:unapproved", "\(mode)")
        }
    }

    // MARK: decision (e): the installs that existed are trusted once

    /// Not started since #3394: served without a stored registration.
    private func stripped(_ fx: TrustFixture, _ name: String) throws -> ServedInstall { try fx.served(["key": name, "registration": "none"]) }

    private func rootOf(_ fx: TrustFixture, _ name: String) throws -> ServedApproval {
        let subject = try fx.served(["key": name])
        return ServedApproval(kind: .root, subjectEndpointId: subject.endpointId, approverEndpointId: subject.endpointId,
                              signature: try InstallApproval.sign(.root, subject: subject, identity: try fx.keys.identity(name), crypto: fx.crypto))
    }

    private func approvalOf(_ fx: TrustFixture, _ name: String, by approver: String) throws -> ServedApproval {
        let subject = try fx.served(["key": name])
        return ServedApproval(kind: .approve, subjectEndpointId: subject.endpointId, approverEndpointId: try fx.keys.endpoint(approver),
                              signature: try InstallApproval.sign(.approve, subject: subject, identity: try fx.keys.identity(approver), crypto: fx.crypto))
    }

    /// An install that EXISTED: its identity is there, its pin store is not yet.
    private func existingInstall(_ fx: TrustFixture, _ name: String, _ api: InstallTrustApi, mode: TrustMode = .record) async throws -> InstallTrustKeeper {
        let identity = try fx.keys.identity(name)
        let keeper = InstallTrustKeeper(api: api, crypto: fx.crypto, persistence: .memory(), identity: { identity }, mode: mode)
        let existing = await keeper.store(accountId: account)?.existing
        XCTAssertEqual(existing, true, name)
        return keeper
    }

    func testDayOneNoRowNoWaitingTheFirstClaimsWithEveryApprovalTheSecondJoinsOnItsChain() async throws {
        let fx = try TrustFixture()
        let server = AccountServer([try fx.served(["key": "A"]), try fx.served(["key": "B"]), try stripped(fx, "L")])
        let a = try await existingInstall(fx, "A", server)
        await a.sync(accountId: account, installId: "install-A")
        let aSnapshot = await a.snapshot(accountId: account)
        XCTAssertEqual(aSnapshot.state, .trusted)
        XCTAssertEqual(aSnapshot.rows, [])
        let stored = server.approvals.value.map { "\($0.kind.rawValue):\(fx.keys.name(of: $0.subjectEndpointId) ?? "?")" }
        XCTAssertEqual(stored.sorted(), ["approve:B", "approve:L", "root:A"])
        let b = try await existingInstall(fx, "B", server)
        await b.sync(accountId: account, installId: "install-B")
        let bSnapshot = await b.snapshot(accountId: account)
        XCTAssertEqual(bSnapshot.state, .trusted)
        XCTAssertEqual(bSnapshot.rows, [])
        let l = await b.store(accountId: account)?.pins[try fx.keys.endpoint("L")]
        XCTAssertEqual(l?.via, .approved)
        XCTAssertEqual(l?.by, try fx.keys.endpoint("A"))
        XCTAssertEqual(server.asked.value, [])
        XCTAssertEqual(server.claims.value, ["install-A"])
    }

    func testTwoExistingInstallsAtOnceTheSecondConflictsAndNeverWaits() async throws {
        let fx = try TrustFixture()
        let server = AccountServer([try fx.served(["key": "A"]), try fx.served(["key": "B"])])
        // Both read "no chain" before either claims.
        server.claimGate = 2
        let a = try await existingInstall(fx, "A", server)
        let b = try await existingInstall(fx, "B", server)
        let states = Locked<[String]>([])
        let accountId = account
        async let first: Void = a.sync(accountId: accountId, installId: "install-A")
        async let second: Void = b.sync(accountId: accountId, installId: "install-B")
        _ = await (first, second)
        for keeper in [a, b] {
            let snapshot = await keeper.snapshot(accountId: account)
            states.mutate { $0.append(snapshot.state.rawValue) }
            XCTAssertEqual(snapshot.rows, [])
        }
        XCTAssertEqual(states.value, ["trusted", "trusted"])
        XCTAssertEqual(server.claims.value.count, 2)
        // A waiting state would have asked.
        XCTAssertEqual(server.asked.value, [])
        XCTAssertEqual(server.approvals.value.filter { $0.kind == .root }.count, 1)
    }

    func testAZPlantedAfterTheFirstReconcileIsARowNeverWrappedNeverApproved() async throws {
        let fx = try TrustFixture()
        let real = Data((0x20..<0x40).map { UInt8($0) })
        for mode in TrustMode.allCases {
            let server = AccountServer([try fx.served(["key": "A"]), try fx.served(["key": "B"]), try stripped(fx, "L")])
            let a = try await existingInstall(fx, "A", server, mode: mode)
            await a.sync(accountId: account, installId: "install-A")
            let consumed = await a.store(accountId: account)?.existing
            XCTAssertEqual(consumed, false, "\(mode)")
            // The lie: Z silent (never asked, no root), as an install from before the release would be.
            let z = try fx.served(["key": "Z"])
            server.installs.mutate { $0.append(z) }
            await a.sync(accountId: account, installId: "install-A")
            let pinned = await a.store(accountId: account)?.pins[z.endpointId]
            XCTAssertNil(pinned, "\(mode)")
            let rows = await a.snapshot(accountId: account).rows
            XCTAssertEqual(rows.map { "\($0.kind.rawValue):\($0.installId)" }, ["newInstall:install-Z"], "\(mode)")
            let keys = LyingServer()
            keys.pendingRows.mutate {
                $0 = [PendingTreeKey(
                    rootId: "01234567-89ab-4def-8123-456789abcdef", epoch: 1, publicKey: TreePublicKeys.of(secret: real)!,
                    targetInstallId: "install-Z", targetX25519Public: z.x25519Public, targetEndpointId: z.endpointId
                )]
            }
            let ring = SealedKeyRing(persistence: .memory)
            ring.add([("01234567-89ab-4def-8123-456789abcdef", 1, real)])
            let identity = try fx.keys.identity("A")
            let holder = TreeKeyHolder(keys: keys, installs: keys, crypto: sealed, trust: a, ring: ring, identity: { identity }, installId: { _ in "install-A" })
            await holder.sync(accountId: account)
            XCTAssertEqual(keys.wrapped.value.map(\.targetInstallId), [], "\(mode)")
            let decided = await a.decisions.last { $0.rule == .r4 && $0.subject == "install-Z" }
            XCTAssertEqual(decided?.verdict, "refused:unapproved", "\(mode)")
            // And it never approved Z by itself.
            XCTAssertEqual(server.approvals.value.filter { $0.subjectEndpointId == z.endpointId }.count, 0, "\(mode)")
        }
    }

    func testAHiddenChainAfterTheFirstReconcileWaits() async throws {
        let fx = try TrustFixture()
        let server = AccountServer([try fx.served(["key": "A"]), try fx.served(["key": "B"])])
        server.approvals.mutate { $0 = [try! self.rootOf(fx, "A")] }
        let b = try await existingInstall(fx, "B", server)
        await b.sync(accountId: account, installId: "install-B")
        // Existing on this install, but not on its account's chain: new.
        let first = await b.snapshot(accountId: account).state
        XCTAssertEqual(first, .waiting)
        let consumed = await b.store(accountId: account)?.existing
        XCTAssertEqual(consumed, false)
        server.approvals.mutate { $0 = [] }
        server.installs.mutate { $0.append(try! fx.served(["key": "Z"])) }
        await b.sync(accountId: account, installId: "install-B")
        let state = await b.snapshot(accountId: account).state
        XCTAssertEqual(state, .waiting)
        let pins = await b.store(accountId: account)?.pins.keys.sorted()
        XCTAssertEqual(pins, [try fx.keys.endpoint("B")])
        XCTAssertEqual(server.claims.value, [])
    }

    /// A new identity, or a new account signed into the one this install
    /// holds: its store is written BEFORE it registers (`markNew`), so it
    /// is a new install: it waits and asks, it adopts nobody.
    func testANewIdentityOrANewAccountWritesItsStoreFirstAndWaits() async throws {
        let fx = try TrustFixture()
        let identity = try fx.keys.identity("N")
        let persistence = TrustStorePersistence.memory()
        InstallTrustKeeper.markNew(accountId: account, identity: identity, crypto: fx.crypto, persistence: persistence)
        let server = AccountServer([try fx.served(["key": "A"]), try fx.served(["key": "B"]), try fx.served(["key": "N"])])
        server.approvals.mutate { $0 = [try! self.rootOf(fx, "A"), try! self.approvalOf(fx, "B", by: "A")] }
        let n = InstallTrustKeeper(api: server, crypto: fx.crypto, persistence: persistence, identity: { identity })
        let existing = await n.store(accountId: account)?.existing
        XCTAssertEqual(existing, false)
        await n.sync(accountId: account, installId: "install-N")
        let state = await n.snapshot(accountId: account).state
        XCTAssertEqual(state, .waiting)
        XCTAssertEqual(server.asked.value, ["install-N"])
        // Next to older builds only (no chain, not alone) it waits too, never adopts.
        let older = AccountServer([try stripped(fx, "A"), try fx.served(["key": "N"])])
        let other = TrustStorePersistence.memory()
        InstallTrustKeeper.markNew(accountId: account, identity: identity, crypto: fx.crypto, persistence: other)
        let fresh = InstallTrustKeeper(api: older, crypto: fx.crypto, persistence: other, identity: { identity })
        await fresh.sync(accountId: account, installId: "install-N")
        let waits = await fresh.snapshot(accountId: account).state
        XCTAssertEqual(waits, .waiting)
        XCTAssertEqual(older.claims.value, [])
    }

    /// `markNew` keeps a store this endpoint already keeps, replaces one kept
    /// for another endpoint (a rotated identity) and never writes over a
    /// store it cannot read.
    func testMarkNewKeepsItsOwnStoreAndNeverWritesOverAnUnreadableOne() throws {
        let fx = try TrustFixture()
        let persistence = TrustStorePersistence.memory()
        var kept = try fx.namedStore("victim").store
        kept.existing = false
        persistence.save(account, try JSONEncoder().encode(kept))
        InstallTrustKeeper.markNew(accountId: account, identity: try fx.keys.identity("A"), crypto: fx.crypto, persistence: persistence)
        XCTAssertEqual(try JSONDecoder().decode(TrustStore.self, from: try XCTUnwrap(try persistence.load(account))), kept)
        InstallTrustKeeper.markNew(accountId: account, identity: try fx.keys.identity("N"), crypto: fx.crypto, persistence: persistence)
        let replaced = try JSONDecoder().decode(TrustStore.self, from: try XCTUnwrap(try persistence.load(account)))
        XCTAssertEqual(replaced, TrustStore(selfEndpointId: try fx.keys.endpoint("N"), userId: ""))
        struct Unreadable: Error {}
        let saved = Locked(0)
        let unreadable = TrustStorePersistence(load: { _ in throw Unreadable() }, save: { _, _ in saved.mutate { $0 += 1 } }, removeAll: {})
        InstallTrustKeeper.markNew(accountId: account, identity: try fx.keys.identity("N"), crypto: fx.crypto, persistence: unreadable)
        XCTAssertEqual(saved.value, 0)
    }

    /// A store that cannot be read (a locked phone) is never taken for none:
    /// no adoption, no claim, no ask, nothing written over it.
    func testAnUnreadableStoreIsNeverTakenForNone() async throws {
        let fx = try TrustFixture()
        struct Unreadable: Error {}
        let saved = Locked(0)
        let persistence = TrustStorePersistence(load: { _ in throw Unreadable() }, save: { _, _ in saved.mutate { $0 += 1 } }, removeAll: {})
        let server = AccountServer([try fx.served(["key": "A"]), try fx.served(["key": "B"])])
        let identity = try fx.keys.identity("A")
        let keeper = InstallTrustKeeper(api: server, crypto: fx.crypto, persistence: persistence, identity: { identity })
        let existing = await keeper.store(accountId: account)?.existing
        XCTAssertEqual(existing, false)
        await keeper.sync(accountId: account, installId: "install-A")
        await keeper.record(accountId: account, rootId: "01234567-89ab-4def-8123-456789abcdef", epoch: 1, record: .verified)
        XCTAssertEqual(server.claims.value, [])
        XCTAssertEqual(server.asked.value, [])
        XCTAssertEqual(saved.value, 0)
    }

    /// A kept store that does not decode, or one kept for another endpoint
    /// (a rotated identity), is a new install's: never existing.
    func testAStoreOfAnotherEndpointOrThatDoesNotDecodeIsNew() async throws {
        let fx = try TrustFixture()
        let identity = try fx.keys.identity("A")
        for data in [Data("{".utf8), try JSONEncoder().encode(TrustStore(selfEndpointId: try fx.keys.endpoint("N"), userId: "user-me", existing: true))] {
            let persistence = TrustStorePersistence.memory()
            persistence.save(account, data)
            let keeper = InstallTrustKeeper(api: AccountServer([]), crypto: fx.crypto, persistence: persistence, identity: { identity })
            let existing = await keeper.store(accountId: account)?.existing
            XCTAssertEqual(existing, false)
        }
    }
}
#endif
