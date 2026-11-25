@testable import ExpCore
import Foundation
import XCTest

// #EXP-2123 (install-trust.json `keyMaker`): only a JOINED install makes a
// chat key. The REAL holder (`TreeKeyHolder` + `InstallTrustKeeper` +
// `SendSealing`, the full eNet binding) of an install that WAITS for
// approval, sending the first message of a new chat, the first message into a
// keyless chat and racing a due rotation, against an HONEST server double (it
// refuses a waiting install's key writes and names it nothing due) and a
// MALICIOUS one (it accepts every key write and names every root as due and
// stranded). Plus the repair of a stranded root. Web: key-maker.test.ts.
#if ENET_NODE
final class KeyMakerTests: XCTestCase {
    private static let root = "01234567-89ab-4def-8123-456789abcdef"
    private static let tree = Data((0x20..<0x40).map { UInt8($0) })
    private static let deviceTree = Data((0x40..<0x60).map { UInt8($0) })
    private let account = "acct"
    private let sealed = EnetSealedCrypto()

    private enum Kind: CaseIterable { case honest, malicious }

    private final class Server: SessionKeysRegistry, TreeReaderDirectory, InstallTrustApi, @unchecked Sendable {
        let kind: Kind
        let waiting = Locked<Set<String>>([])
        let trustAnswer: Locked<TrustAnswer>
        let dueRows = Locked<[DueTreeKey]>([])
        /// The server's `due` lags behind its refusals (names them anyway).
        let dueLags = Locked(false)
        let mineRows = Locked<[HeldTreeKey]>([])
        let pendingRows = Locked<[(target: String, x25519: String, endpoint: String)]>([])
        let deviceKey = Locked<String?>(nil)
        let trustCalls = Locked(0)
        let offline = Locked(false)
        let older = Locked(false)
        let created = Locked<[CreateTreeKeyInput]>([])
        let refused = Locked<[CreateTreeKeyInput]>([])
        let wrapped = Locked<[PendingTreeKeyWrap]>([])
        let asked = Locked<[String]>([])
        let sent = Locked<[SealedSendInput]>([])

        init(_ kind: Kind, _ trust: TrustAnswer) {
            self.kind = kind
            trustAnswer = Locked(trust)
        }

        private func refuses(_ installId: String) -> Bool { kind == .honest && waiting.value.contains(installId) }
        static let refusal = ClearTreeFixtureTests.trpc(403, "FORBIDDEN", keyMakerRefusal)

        func sendTarget(accountId: String, agentId: String?, appId: String?) async throws -> ServerSendTarget {
            ServerSendTarget(rootId: nil, epoch: nil, publicKey: nil)
        }
        func create(accountId: String, input: CreateTreeKeyInput) async throws {
            if refuses(input.installId) {
                refused.mutate { $0.append(input) }
                throw Self.refusal
            }
            created.mutate { $0.append(input) }
        }
        func mine(accountId: String, installId: String, rootIds: [String]?) async throws -> [HeldTreeKey] {
            mineRows.value.filter { rootIds?.contains($0.rootId) ?? true }
        }
        func pending(accountId: String, installId: String) async throws -> [PendingTreeKey] {
            guard let made = created.value.last else { return [] }
            return pendingRows.value.map {
                PendingTreeKey(rootId: made.rootId, epoch: made.epoch, publicKey: made.publicKey, targetInstallId: $0.target, targetX25519Public: $0.x25519, targetEndpointId: $0.endpoint)
            }
        }
        func wrap(accountId: String, installId: String, wraps: [PendingTreeKeyWrap]) async throws { wrapped.mutate { $0 += wraps } }
        func publicKeys(accountId: String, rootIds: [String]) async throws -> [TreePublicKey] {
            guard let key = deviceKey.value else { return [] }
            return rootIds.map { TreePublicKey(rootId: $0, epoch: 1, publicKey: key) }
        }
        func requestKey(accountId: String, rootId: String) async throws -> RequestKeyResult {
            asked.mutate { $0.append(rootId) }
            deviceKey.mutate { $0 = TreePublicKeys.of(secret: KeyMakerTests.deviceTree) }
            return RequestKeyResult(state: "requested")
        }
        func due(accountId: String, installId: String) async throws -> [DueTreeKey] {
            refuses(installId) && !dueLags.value ? [] : dueRows.value
        }
        func resign(accountId: String, installId: String, wraps: [ResignedTreeKeyWrap]) async throws -> ResignResult {
            ResignResult(replaced: wraps.count, skipped: 0)
        }

        func mine(accountId: String) async throws -> [InstallEntry] {
            trustAnswer.value.installs.map {
                InstallEntry(installId: $0.installId, userId: $0.userId, kind: "ios", endpointId: $0.endpointId, x25519Public: $0.x25519Public)
            }
        }
        func list(accountId: String, teamId: String) async throws -> [InstallEntry] { [] }

        func trust(accountId: String, installId: String) async throws -> TrustAnswer {
            trustCalls.mutate { $0 += 1 }
            if offline.value { throw URLError(.notConnectedToInternet) }
            if older.value { throw ClearTreeFixtureTests.notFound }
            return trustAnswer.value
        }
        func requestApproval(accountId: String, installId: String) async throws {}
        func claimRoot(accountId: String, installId: String, signature: String, approvals: [RootClaimApproval]?) async throws {}
        func approve(accountId: String, installId: String, subjectInstallId: String, kind: ApprovalKind, signature: String) async throws {}
        func revoke(accountId: String, installId: String) async throws {}

        /// `sessions.send`: an inline seal is a key write like any other.
        func send(_ input: SealedSendInput) throws {
            if let seal = input.seal, refuses(seal.installId) { throw Self.refusal }
            sent.mutate { $0.append(input) }
        }
    }

    private struct Install {
        let server: Server
        let keeper: InstallTrustKeeper
        let holder: TreeKeyHolder
        let sending: SendSealing
        let ring: SealedKeyRing

        func send(_ account: String, _ input: SessionsSendInput, root: SealRoot? = nil) async throws {
            try await StaleTreeResend.send { fresh in
                let prepared = try await sending.prepare(accountId: account, input, root: root, mentions: nil, deviceIds: { _ in [] }, fresh: fresh)
                try server.send(prepared.input)
            }
        }
    }

    private func approval(_ fx: TrustFixture, _ kind: ApprovalKind, _ name: String, by approver: String) throws -> ServedApproval {
        let subject = try fx.served(["key": name])
        return ServedApproval(
            kind: kind, subjectEndpointId: subject.endpointId, approverEndpointId: try fx.keys.endpoint(approver),
            signature: try InstallApproval.sign(kind, subject: subject, identity: try fx.keys.identity(approver), crypto: fx.crypto)
        )
    }

    /// The account: A started the chain, N registered afterwards and asked.
    private func server(_ fx: TrustFixture, _ kind: Kind, approved: Bool = false) throws -> Server {
        var chain = [try approval(fx, .root, "A", by: "A")]
        if approved { chain.append(try approval(fx, .approve, "N", by: "A")) }
        let server = Server(kind, TrustAnswer(installs: [try fx.served(["key": "A"]), try fx.served(["key": "N", "approvalRequested": true])], approvals: chain))
        if !approved { server.waiting.mutate { $0 = ["install-N"] } }
        return server
    }

    private func install(_ fx: TrustFixture, _ name: String, _ server: Server) throws -> Install {
        let identity = try fx.keys.identity(name)
        let persistence = TrustStorePersistence.memory()
        InstallTrustKeeper.markNew(accountId: account, identity: identity, crypto: fx.crypto, persistence: persistence)
        let keeper = InstallTrustKeeper(api: server, crypto: fx.crypto, persistence: persistence, identity: { identity })
        let ring = SealedKeyRing(persistence: .memory)
        let holder = TreeKeyHolder(
            keys: server, installs: server, crypto: sealed, signer: WrapSignerFactory.make(), trust: keeper, ring: ring,
            identity: { identity }, installId: { _ in "install-\(name)" }
        )
        let sending = SendSealing(holder: holder, crypto: sealed, holderWait: .zero, keyWait: .milliseconds(50), createRetry: .zero)
        return Install(server: server, keeper: keeper, holder: holder, sending: sending, ring: ring)
    }

    private var keyless: SealRoot { SealRoot(id: Self.root, teamId: "team", deviceId: "mac-1", shared: false, key: nil) }
    private let newChat = SessionsSendInput.newChat(teamId: "team", deviceId: "mac-1", text: "hello")
    private let message = SessionsSendInput.message(sessionId: KeyMakerTests.root, text: "hello")

    private func assertRefused(_ on: Install, _ input: SessionsSendInput, root: SealRoot? = nil, _ label: String) async {
        do {
            try await on.send(account, input, root: root)
            XCTFail("\(label): it was sent")
        } catch {
            XCTAssertEqual(error as? SealRefusal, SealRefusal(), label)
        }
        XCTAssertEqual(on.server.sent.value, [], label)
    }

    /// A wrap of epoch 1 an older build made (unsigned) for `name`.
    private func unsignedWrap(_ fx: TrustFixture, _ name: String) throws -> HeldTreeKey {
        HeldTreeKey(rootId: Self.root, epoch: 1, wrapped: try sealed.wrap(installPublic: try fx.keys.text(name, "x25519Public"), treeSecret: Self.tree))
    }

    // MARK: waiting

    func testWaitingTheFirstMessageOfANewChatFails() async throws {
        let fx = try TrustFixture()
        for kind in Kind.allCases {
            let n = try install(fx, "N", try server(fx, kind))
            // Never reconciled before: reconciled first, then judged.
            await assertRefused(n, newChat, "\(kind)")
            let state = await n.keeper.snapshot(accountId: account).state
            XCTAssertEqual(state, .waiting, "\(kind)")
            await n.holder.sync(accountId: account)
            await assertRefused(n, newChat, "\(kind), after a pass")
            XCTAssertEqual(n.server.created.value, [], "\(kind)")
            XCTAssertEqual(n.server.refused.value, [], "\(kind)")
        }
    }

    func testWaitingTheFirstMessageIntoAKeylessChatAsksItsDevices() async throws {
        let fx = try TrustFixture()
        for kind in Kind.allCases {
            let n = try install(fx, "N", try server(fx, kind))
            try await n.send(account, message, root: keyless)
            XCTAssertEqual(n.server.created.value, [], "\(kind)")
            XCTAssertEqual(n.server.refused.value, [], "\(kind)")
            XCTAssertEqual(n.server.asked.value, [Self.root], "\(kind)")
            let sent = try XCTUnwrap(n.server.sent.value.first)
            XCTAssertNil(sent.seal, "\(kind)")
            let text = try XCTUnwrap(sent.input.text)
            XCTAssertTrue(SealedEnvelope.isSealedBody(text), "\(kind)")
            XCTAssertEqual(sealed.openBody(text, keys: [1: Self.deviceTree]).text, "hello", "\(kind)")
            XCTAssertTrue(n.holder.held(rootId: Self.root).isEmpty, "\(kind)")
        }
    }

    /// `sealKey` (a notification's reply, a sealed card's answer).
    func testWaitingSealKeyAsksTheDevices() async throws {
        let fx = try TrustFixture()
        for kind in Kind.allCases {
            let n = try install(fx, "N", try server(fx, kind))
            let key = await n.holder.sealKey(accountId: account, rootId: Self.root, known: nil, teamId: "team", deviceIds: [], shared: false, keyWait: .milliseconds(50))
            XCTAssertEqual(key?.publicKey, TreePublicKeys.of(secret: Self.deviceTree), "\(kind)")
            XCTAssertEqual(n.server.asked.value, [Self.root], "\(kind)")
            XCTAssertEqual(n.server.created.value, [], "\(kind)")
            XCTAssertEqual(n.server.refused.value, [], "\(kind)")
        }
    }

    func testWaitingNeverWinsADueRotation() async throws {
        let fx = try TrustFixture()
        for kind in Kind.allCases {
            let n = try install(fx, "N", try server(fx, kind))
            let wrap = try unsignedWrap(fx, "N")
            n.server.mineRows.mutate { $0 = [wrap] }
            n.server.dueRows.mutate { $0 = [DueTreeKey(rootId: Self.root, epoch: 1), DueTreeKey(rootId: Self.root, epoch: 1, stranded: true)] }
            for _ in 0..<3 { await n.holder.sync(accountId: account) }
            let state = await n.keeper.snapshot(accountId: account).state
            XCTAssertEqual(state, .waiting, "\(kind)")
            XCTAssertEqual(n.holder.held(rootId: Self.root), [1: Self.tree], "\(kind): it holds the current epoch")
            XCTAssertEqual(n.server.created.value, [], "\(kind)")
            XCTAssertEqual(n.server.refused.value, [], "\(kind)")
        }
    }

    /// A trust store that cannot be reconciled (offline) makes nothing.
    func testUnknownMakesNothingAndAnOlderServerCountsAsJoined() async throws {
        let fx = try TrustFixture()
        let n = try install(fx, "N", try server(fx, .malicious))
        n.server.offline.mutate { $0 = true }
        await assertRefused(n, newChat, "offline")
        XCTAssertGreaterThan(n.server.trustCalls.value, 0)
        let older = try install(fx, "N", try server(fx, .malicious))
        older.server.older.mutate { $0 = true }
        try await older.send(account, newChat)
        XCTAssertNotNil(older.server.sent.value.first?.seal)
    }

    // MARK: approved, and the server's lag

    func testOnceApprovedItKeysAndRotates() async throws {
        let fx = try TrustFixture()
        let n = try install(fx, "N", try server(fx, .honest))
        let wrap = try unsignedWrap(fx, "N")
        n.server.mineRows.mutate { $0 = [wrap] }
        n.server.dueRows.mutate { $0 = [DueTreeKey(rootId: Self.root, epoch: 1)] }
        await n.holder.sync(accountId: account)
        await assertRefused(n, newChat, "waiting")
        // A approves it.
        let approve = try approval(fx, .approve, "N", by: "A")
        n.server.trustAnswer.mutate { $0 = TrustAnswer(installs: $0.installs, approvals: $0.approvals + [approve]) }
        n.server.waiting.mutate { $0 = [] }
        await n.holder.sync(accountId: account)
        XCTAssertEqual(n.server.created.value.map(\.epoch), [2])
        try await n.send(account, newChat)
        let seal = try XCTUnwrap(n.server.sent.value.first?.seal)
        XCTAssertEqual(Set(seal.wraps.map(\.installId)), ["install-A", "install-N"])
        let other = "41234567-89ab-4def-8123-456789abcdef"
        try await n.send(account, .message(sessionId: other, text: "hi"), root: SealRoot(id: other, teamId: "team", deviceId: "mac-1", shared: false, key: nil))
        XCTAssertEqual(n.server.created.value.map(\.epoch), [2, 1])
        XCTAssertEqual(n.server.asked.value, [])
    }

    func testALaggingServersRotationRefusalIsNotRemembered() async throws {
        let fx = try TrustFixture()
        let n = try install(fx, "N", try server(fx, .honest, approved: true))
        n.server.waiting.mutate { $0 = ["install-N"] }
        n.server.dueLags.mutate { $0 = true }
        let wrap = try unsignedWrap(fx, "N")
        n.server.mineRows.mutate { $0 = [wrap] }
        n.server.dueRows.mutate { $0 = [DueTreeKey(rootId: Self.root, epoch: 1)] }
        await n.holder.sync(accountId: account)
        await n.holder.sync(accountId: account)
        XCTAssertEqual(n.server.refused.value.count, 2)
        XCTAssertEqual(n.server.created.value, [])
        n.server.waiting.mutate { $0 = [] }
        await n.holder.sync(accountId: account)
        XCTAssertEqual(n.server.created.value.map(\.epoch), [2])
    }

    func testALaggingServersRefusalOfAKeylessAndANewChat() async throws {
        let fx = try TrustFixture()
        let n = try install(fx, "N", try server(fx, .honest, approved: true))
        n.server.waiting.mutate { $0 = ["install-N"] }
        await assertRefused(n, newChat, "new chat")
        try await n.send(account, message, root: keyless)
        XCTAssertEqual(n.server.refused.value.count, 1)
        XCTAssertEqual(n.server.asked.value, [Self.root])
        let text = try XCTUnwrap(n.server.sent.value.first?.input.text)
        XCTAssertEqual(sealed.openBody(text, keys: [1: Self.deviceTree]).text, "hello")
    }

    // MARK: the repair

    func testAJoinedInstallRotatesAStrandedRootWithoutItsSecret() async throws {
        let fx = try TrustFixture()
        let b = try fx.served(["key": "B"])
        let server = try server(fx, .honest)
        let approve = try approval(fx, .approve, "B", by: "A")
        server.trustAnswer.mutate { $0 = TrustAnswer(installs: $0.installs + [b], approvals: $0.approvals + [approve]) }
        server.dueRows.mutate { $0 = [DueTreeKey(rootId: Self.root, epoch: 1, stranded: true)] }
        server.pendingRows.mutate { $0 = [("install-B", b.x25519Public, b.endpointId)] }
        // The install that started the chain (its root claim is what `installs.trust` serves).
        let a = try install(fx, "A", server)
        await a.holder.sync(accountId: account)
        let state = await a.keeper.snapshot(accountId: account).state
        XCTAssertEqual(state, .trusted)
        let made = try XCTUnwrap(server.created.value.first)
        XCTAssertEqual(made.epoch, 2)
        XCTAssertEqual(made.wraps.map(\.installId), ["install-A"])
        XCTAssertNotNil(made.wraps.first?.signature)
        XCTAssertEqual(a.holder.held(rootId: Self.root).keys.sorted(), [2])
        XCTAssertEqual(server.wrapped.value.map(\.targetInstallId), ["install-B"])
    }

    func testADueRootNotStrandedAndNotHeldStaysAnotherHolders() async throws {
        let fx = try TrustFixture()
        let server = try server(fx, .honest)
        server.dueRows.mutate { $0 = [DueTreeKey(rootId: Self.root, epoch: 1)] }
        let a = try install(fx, "A", server)
        await a.holder.sync(accountId: account)
        XCTAssertEqual(server.created.value, [])
    }
}
#endif
