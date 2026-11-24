import CryptoKit
import Foundation

// #EXP-1950 (packages/domain-contract/fixtures/install-trust.json, read its
// `$comment`; TS reference `src/install-trust.ts`): which installs this
// install TRUSTS, and what it does with a key the server hands it.
//
// - THE CODE an install shows while it waits for approval: `InstallCode`.
// - THE APPROVALS, signed by an install's eNet identity: `InstallApproval`.
// - THE PIN STORE of one install per account and how it follows
//   `installs.trust`: `InstallTrust.reconcile`. The server only ever SHRINKS
//   the pins; they grow only by a signature this install checks itself.
// - THE FOUR RULES, record-only in release 1 (`TrustMode.build`): `keepWrap`
//   (R1), `sealEpochCheck` (R2), `sealTo` (R3), `wrapTarget` (R4) and
//   `callKey`. Each answers what a build does NOW (and records) and once
//   signing is mandatory (decision d).
// The ed25519 runs through the eNet binding (`TrustCrypto`, strict verify =
// `identityVerify`); SHA-256 and X25519 public halves are CryptoKit's.

/// Release 1 = `record`: nothing a released build makes is refused;
/// `mandatory` = release 2 of decision (d). A BUILD constant, never a server flag.
public enum TrustMode: String, Sendable, CaseIterable {
    case record, mandatory

    public static let build: TrustMode = .record
}

/// What this install recorded about a key it holds.
public enum KeyRecord: String, Codable, Sendable {
    case verified, legacy, unsigned
    case refusedWouldBe = "refused-would-be"
}

/// Why an install is pinned: `itself` = this install ("self"); `chain` = on
/// the path from its account's root when it joined; `approved` = an
/// `approve` by a pinned install of the same account; `first` = a
/// team-mate's root at first contact; `reset` = a team-mate's new root a
/// pinned install confirmed.
public enum PinVia: String, Codable, Sendable {
    case itself = "self"
    case chain, approved, first, reset
}

public struct TrustPin: Codable, Equatable, Sendable {
    public var userId: String
    public var x25519Public: String
    public var via: PinVia
    /// The approver (`approved`, `reset`); nil otherwise.
    public var by: String?

    public init(userId: String, x25519Public: String, via: PinVia, by: String?) {
        self.userId = userId
        self.x25519Public = x25519Public
        self.via = via
        self.by = by
    }
}

public struct TrustRootRecord: Codable, Equatable, Sendable {
    public var verifiedEpoch: UInt32
    /// epoch (decimal) → record.
    public var epochs: [String: KeyRecord]

    public init(verifiedEpoch: UInt32 = 0, epochs: [String: KeyRecord] = [:]) {
        self.verifiedEpoch = verifiedEpoch
        self.epochs = epochs
    }
}

/// The pin store of one install for one account (the Keychain, beside the
/// tree keys: `InstallTrustState`'s persistence).
public struct TrustStore: Codable, Equatable, Sendable {
    /// This install's endpoint id.
    public var selfEndpointId: String
    public var userId: String
    /// Placed in its account's chain (a root, or approved).
    public var joined: Bool
    public var pins: [String: TrustPin]
    /// Every team-mate ever pinned: a new root of one is a Reset, not a first contact.
    public var members: [String]
    /// Endpoint ids this install's account turned down (a Reset row's Remove).
    public var declined: [String]
    /// Per root (lowercase): the highest VERIFIED epoch and what each held epoch is.
    public var roots: [String: TrustRootRecord]

    enum CodingKeys: String, CodingKey {
        case selfEndpointId = "self"
        case userId, joined, pins, members, declined, roots
    }

    public init(selfEndpointId: String, userId: String) {
        self.selfEndpointId = selfEndpointId
        self.userId = userId
        joined = false
        pins = [:]
        members = []
        declined = []
        roots = [:]
    }

    /// The record of a held key; a key the store never saw = `legacy` (decision c).
    public func record(rootId: String, epoch: UInt32) -> KeyRecord {
        roots[rootId.lowercased()]?.epochs[String(epoch)] ?? .legacy
    }

    /// Records a kept key (`verified` raises the root's verified epoch).
    public mutating func recordKey(rootId: String, epoch: UInt32, record: KeyRecord) {
        let root = rootId.lowercased()
        var entry = roots[root] ?? TrustRootRecord()
        if record == .verified { entry.verifiedEpoch = max(entry.verifiedEpoch, epoch) }
        entry.epochs[String(epoch)] = record
        roots[root] = entry
    }
}

// ── what the server serves (`installs.trust`) ─────────────────────────────

/// A live install of the viewer's account or of a team-mate.
public struct ServedInstall: Codable, Equatable, Sendable {
    public var installId: String
    public var userId: String
    public var endpointId: String
    public var x25519Public: String
    public var registrationSignature: String?
    public var registrationSignedAt: String?
    /// A build that follows the rule asked to be approved (`installs.requestApproval`).
    public var approvalRequested: Bool
    public var label: String?

    public init(
        installId: String, userId: String, endpointId: String, x25519Public: String,
        registrationSignature: String?, registrationSignedAt: String?, approvalRequested: Bool, label: String? = nil
    ) {
        self.installId = installId
        self.userId = userId
        self.endpointId = endpointId
        self.x25519Public = x25519Public
        self.registrationSignature = registrationSignature
        self.registrationSignedAt = registrationSignedAt
        self.approvalRequested = approvalRequested
        self.label = label
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        installId = try c.decode(String.self, forKey: .installId)
        userId = try c.decode(String.self, forKey: .userId)
        endpointId = try c.decode(String.self, forKey: .endpointId)
        x25519Public = try c.decode(String.self, forKey: .x25519Public)
        registrationSignature = try c.decodeIfPresent(String.self, forKey: .registrationSignature)
        registrationSignedAt = try c.decodeIfPresent(String.self, forKey: .registrationSignedAt)
        approvalRequested = try c.decodeIfPresent(Bool.self, forKey: .approvalRequested) ?? false
        label = try c.decodeIfPresent(String.self, forKey: .label)
    }
}

/// What an approval says (`approval.kinds`).
public enum ApprovalKind: String, Codable, Sendable, CaseIterable {
    case root, approve, reset, decline

    public var byte: UInt8 {
        switch self {
        case .root: 1
        case .approve: 2
        case .reset: 3
        case .decline: 4
        }
    }
}

public struct ServedApproval: Codable, Equatable, Sendable {
    public var kind: ApprovalKind
    public var subjectEndpointId: String
    public var approverEndpointId: String
    public var signature: String

    public init(kind: ApprovalKind, subjectEndpointId: String, approverEndpointId: String, signature: String) {
        self.kind = kind
        self.subjectEndpointId = subjectEndpointId
        self.approverEndpointId = approverEndpointId
        self.signature = signature
    }
}

/// `installs.trust`'s answer.
public struct TrustAnswer: Codable, Equatable, Sendable {
    public var installs: [ServedInstall]
    public var approvals: [ServedApproval]

    public init(installs: [ServedInstall], approvals: [ServedApproval]) {
        self.installs = installs
        self.approvals = approvals
    }
}

/// A row of Needs you: approve or remove an install.
public struct TrustRow: Equatable, Sendable, Identifiable {
    public enum Kind: String, Sendable { case newInstall, reset }

    public let kind: Kind
    public let installId: String
    public let endpointId: String
    public let userId: String
    public let label: String?
    /// Derived HERE from `endpointId`, never read from the server.
    public let code: String

    public var id: String { installId }

    public init(kind: Kind, installId: String, endpointId: String, userId: String, label: String?, code: String) {
        self.kind = kind
        self.installId = installId
        self.endpointId = endpointId
        self.userId = userId
        self.label = label
        self.code = code
    }
}

/// `trusted` = joined; `waiting` = shows "Waiting for approval" and its
/// code; `claimRoot` = its account has no chain: it signs `root` for itself.
public enum TrustStatus: String, Sendable { case trusted, waiting, claimRoot }

public struct TrustView: Equatable, Sendable {
    public var store: TrustStore
    public var status: TrustStatus
    public var rows: [TrustRow]
}

// ── the crypto (the eNet binding; a fake never stands in for the replays) ─

/// Why a wrap (or a vouch) is not kept: wrap-signature.json `refusals`.
public enum WrapRefusal: String, Sendable { case malformed, unsigned, unpinned, unopened, signature }

public enum WrapAcceptance: Equatable, Sendable {
    case verified(secret: Data, treePublicKey: String)
    case unsigned(secret: Data, treePublicKey: String)
    case refused(WrapRefusal)
}

public protocol TrustCrypto: Sendable {
    /// Strict ed25519: `signature` (base64url, 86 chars) by `endpointId` over `message`. Never throws.
    func verify(signature: String, message: Data, endpointId: String) -> Bool
    /// The identity's signature over `message`, base64url without padding.
    func sign(identity: Data, message: Data) throws -> String
    func endpointId(identity: Data) throws -> String
    /// wrap-signature.json `acceptWrap` (the binding's).
    func acceptWrap(_ wrap: HeldTreeKey, identity: Data, installSecret: Data, pins: [String], required: Bool) -> WrapAcceptance
    /// wrap-signature.json `acceptVouch`: nil = verified.
    func acceptVouch(_ statement: WrapStatement, signature: String?, signerEndpointId: String?, pins: [String]) -> WrapRefusal?
}

#if ENET_NODE
public struct EnetTrustCrypto: TrustCrypto {
    public init() {}

    public func verify(signature: String, message: Data, endpointId: String) -> Bool {
        guard InstallTrust.isSignature(signature), InstallTrust.isEndpointId(endpointId),
              let bytes = InstallTrust.b64urlDecode(signature), bytes.count == 64 else { return false }
        return identityVerify(endpointId: endpointId, message: message, signature: bytes)
    }

    public func sign(identity: Data, message: Data) throws -> String {
        InstallTrust.b64urlEncode(try identitySign(identity: identity, message: message))
    }

    public func endpointId(identity: Data) throws -> String {
        try endpointIdOf(identity: identity)
    }

    private static func refusal(_ reason: EnetWrapRefusal) -> WrapRefusal {
        switch reason {
        case .malformed: .malformed
        case .unsigned: .unsigned
        case .unpinned: .unpinned
        case .unopened: .unopened
        case .signature: .signature
        }
    }

    public func acceptWrap(_ wrap: HeldTreeKey, identity: Data, installSecret: Data, pins: [String], required: Bool) -> WrapAcceptance {
        let verdict = try? ExpCore.acceptWrap(
            wrap: EnetServedWrap(rootId: wrap.rootId, epoch: wrap.epoch, wrapped: wrap.wrapped, signature: wrap.signature, signerEndpointId: wrap.signerEndpointId),
            identity: identity, installSecret: installSecret, pins: pins, required: required
        )
        switch verdict {
        case let .verified(secret, publicKey, _): return .verified(secret: secret, treePublicKey: publicKey)
        case let .unsigned(secret, publicKey): return .unsigned(secret: secret, treePublicKey: publicKey)
        case let .refused(reason): return .refused(Self.refusal(reason))
        case nil: return .refused(.malformed)
        }
    }

    public func acceptVouch(_ statement: WrapStatement, signature: String?, signerEndpointId: String?, pins: [String]) -> WrapRefusal? {
        let verdict = try? ExpCore.acceptVouch(
            statement: EnetWrapStatement(
                rootId: statement.rootId, epoch: statement.epoch, treePublicKey: statement.treePublicKey,
                recipientEndpointId: statement.recipientEndpointId, wrapped: statement.wrapped
            ),
            signature: signature, signerEndpointId: signerEndpointId, pins: pins
        )
        switch verdict {
        case .verified: return nil
        case let .refused(reason): return Self.refusal(reason)
        case nil: return .malformed
        }
    }
}
#endif

public enum TrustCryptoFactory {
    /// nil without the full binding: the rules are off (as before #EXP-1950).
    public static func make() -> TrustCrypto? {
        #if ENET_NODE
        EnetTrustCrypto()
        #else
        nil
        #endif
    }
}

// ── the code ──────────────────────────────────────────────────────────────

public enum InstallCode {
    public static let label = "exp/enet/install-code/1"
    public static let alphabet = Array("0123456789ABCDEFGHJKMNPQRSTVWXYZ")

    /// The 10 characters (two groups of five) an install shows to be
    /// approved: the first 50 bits of SHA-256(label ‖ 0x00 ‖ endpointId[32])
    /// in Crockford base32. Both ends derive it; nil for a malformed id.
    public static func of(endpointId: String) -> String? {
        guard InstallTrust.isEndpointId(endpointId), let key = InstallTrust.hexBytes(endpointId) else { return nil }
        let digest = Array(SHA256.hash(data: Data(label.utf8) + Data([0]) + key))
        var bits: UInt64 = 0
        for byte in digest.prefix(7) { bits = (bits << 8) | UInt64(byte) }
        bits >>= 6
        var out = ""
        for i in stride(from: 9, through: 0, by: -1) { out.append(alphabet[Int((bits >> UInt64(5 * i)) & 31)]) }
        return "\(out.prefix(5)) \(out.suffix(5))"
    }
}

// ── the approvals, the registration ───────────────────────────────────────

public enum InstallApproval {
    public static let label = "exp/enet/install-approval/1"
    public static let messageBytes = 125

    /// label ‖ 0x00 ‖ kind u8 ‖ SHA-256(userId utf8)[32] ‖ endpointId[32] ‖
    /// x25519Public[32]; nil when malformed.
    public static func message(_ kind: ApprovalKind, userId: String, endpointId: String, x25519Public: String) -> Data? {
        guard InstallTrust.isEndpointId(endpointId), !userId.isEmpty,
              let endpoint = InstallTrust.hexBytes(endpointId),
              let x = InstallTrust.b64urlDecode(x25519Public), x.count == 32 else { return nil }
        var out = Data(label.utf8)
        out.append(0)
        out.append(kind.byte)
        out.append(contentsOf: SHA256.hash(data: Data(userId.utf8)))
        out.append(endpoint)
        out.append(x)
        return out
    }

    public static func message(_ kind: ApprovalKind, subject: ServedInstall) -> Data? {
        message(kind, userId: subject.userId, endpointId: subject.endpointId, x25519Public: subject.x25519Public)
    }

    public static func sign(_ kind: ApprovalKind, subject: ServedInstall, identity: Data, crypto: TrustCrypto) throws -> String {
        guard let message = message(kind, subject: subject) else { throw InstallTrustError.malformedSubject }
        return try crypto.sign(identity: identity, message: message)
    }

    public static func verify(_ kind: ApprovalKind, subject: ServedInstall, signature: String, approverEndpointId: String, crypto: TrustCrypto) -> Bool {
        guard let message = message(kind, subject: subject) else { return false }
        return crypto.verify(signature: signature, message: message, endpointId: approverEndpointId)
    }
}

public enum InstallTrustError: Error, Equatable {
    case malformedSubject
    case notTrusted
    case installNotFound
}

// ── reconcile + the four rules ────────────────────────────────────────────

public enum InstallTrust {
    static func isEndpointId(_ text: String) -> Bool {
        text.utf8.count == 64 && text.utf8.allSatisfy { (48...57).contains($0) || (97...102).contains($0) }
    }

    /// base64url of exactly 64 bytes, canonical (`[A-Za-z0-9_-]{85}[AQgw]`).
    static func isSignature(_ text: String) -> Bool {
        let bytes = Array(text.utf8)
        guard bytes.count == 86 else { return false }
        let ok: (UInt8) -> Bool = { (65...90).contains($0) || (97...122).contains($0) || (48...57).contains($0) || $0 == 45 || $0 == 95 }
        return bytes.prefix(85).allSatisfy(ok) && [65, 81, 103, 119].contains(bytes[85])
    }

    /// install-register.json's message (`InstallRegistration.message`; the
    /// share extension compiles this file without the install node).
    public static func registrationMessage(userId: String, endpointId: String, x25519Public: String, signedAt: String) -> String {
        "exp-install-v1\n\(userId)\n\(endpointId)\n\(x25519Public)\n\(signedAt)"
    }

    static func b64urlEncode(_ data: Data) -> String {
        data.base64EncodedString().replacingOccurrences(of: "+", with: "-").replacingOccurrences(of: "/", with: "_").replacingOccurrences(of: "=", with: "")
    }

    static func b64urlDecode(_ text: String) -> Data? {
        var base64 = text.replacingOccurrences(of: "-", with: "+").replacingOccurrences(of: "_", with: "/")
        let remainder = base64.count % 4
        if remainder == 1 { return nil }
        if remainder > 0 { base64 += String(repeating: "=", count: 4 - remainder) }
        return Data(base64Encoded: base64)
    }

    static func hexBytes(_ hex: String) -> Data? {
        let chars = Array(hex.utf8)
        guard chars.count % 2 == 0 else { return nil }
        var out = Data(capacity: chars.count / 2)
        var i = 0
        while i < chars.count {
            guard let byte = UInt8(String(decoding: chars[i..<i + 2], as: UTF8.self), radix: 16) else { return nil }
            out.append(byte)
            i += 2
        }
        return out
    }

    /// Whether the row's stored registration binds its user, endpoint id and
    /// X25519 key (install-register.json's message).
    public static func registrationValid(_ install: ServedInstall, crypto: TrustCrypto) -> Bool {
        guard let signature = install.registrationSignature, let signedAt = install.registrationSignedAt else { return false }
        let text = registrationMessage(userId: install.userId, endpointId: install.endpointId, x25519Public: install.x25519Public, signedAt: signedAt)
        return crypto.verify(signature: signature, message: Data(text.utf8), endpointId: install.endpointId)
    }

    /// Follows the server's answer. The server may only SHRINK the pins; a
    /// pin is ADDED only for a signature this install verified, by an install
    /// already pinned (or, once, at joining and at first contact).
    public static func reconcile(_ before: TrustStore, served: TrustAnswer, crypto: TrustCrypto) -> TrustView {
        var store = before
        // The served installs whose registration verifies, by endpoint id (first position kept).
        var valid: [String: ServedInstall] = [:]
        var order: [String] = []
        for install in served.installs where registrationValid(install, crypto: crypto) {
            if valid[install.endpointId] == nil { order.append(install.endpointId) }
            valid[install.endpointId] = install
        }
        let me = valid[store.selfEndpointId]

        // Shrink: what the server no longer lists (or binds) is no longer pinned.
        for (endpointId, pin) in store.pins where endpointId != store.selfEndpointId {
            guard let row = valid[endpointId], row.userId == pin.userId, row.x25519Public == pin.x25519Public else {
                store.pins[endpointId] = nil
                continue
            }
        }
        if let me { store.pins[store.selfEndpointId] = TrustPin(userId: store.userId, x25519Public: me.x25519Public, via: .itself, by: nil) }

        // The approvals that verify, against the subject as its registration binds it.
        let approvals = served.approvals.filter { approval in
            guard let subject = valid[approval.subjectEndpointId] else { return false }
            if approval.kind == .root, approval.approverEndpointId != approval.subjectEndpointId { return false }
            return InstallApproval.verify(approval.kind, subject: subject, signature: approval.signature, approverEndpointId: approval.approverEndpointId, crypto: crypto)
        }
        let roots = Set(approvals.filter { $0.kind == .root }.map(\.subjectEndpointId))
        let userOf: (String) -> String? = { valid[$0]?.userId }

        // Joining: its own root claim, or the chain from its account's root to it.
        if !store.joined, me != nil {
            if roots.contains(store.selfEndpointId) {
                store.joined = true
            } else if let path = chain(to: store.selfEndpointId, userId: store.userId, approvals: approvals, roots: roots, userOf: userOf) {
                for endpointId in path where endpointId != store.selfEndpointId {
                    let row = valid[endpointId]!
                    store.pins[endpointId] = TrustPin(userId: row.userId, x25519Public: row.x25519Public, via: .chain, by: nil)
                }
                store.joined = true
            }
        }

        // Declines by this account's pinned installs.
        for approval in approvals where approval.kind == .decline {
            if store.pins[approval.approverEndpointId]?.userId == store.userId, !store.declined.contains(approval.subjectEndpointId) {
                store.declined.append(approval.subjectEndpointId)
            }
        }

        if store.joined {
            // First contact with a team-mate: their root(s) as served, once.
            for endpointId in order {
                let install = valid[endpointId]!
                if install.userId == store.userId || store.members.contains(install.userId) { continue }
                if store.pins.values.contains(where: { $0.userId == install.userId }) { continue }
                for rootId in order {
                    let root = valid[rootId]!
                    if root.userId == install.userId, roots.contains(rootId), !store.declined.contains(rootId) {
                        store.pins[rootId] = TrustPin(userId: root.userId, x25519Public: root.x25519Public, via: .first, by: nil)
                    }
                }
            }
            // Grow by signatures of pinned installs, to a fixpoint.
            var changed = true
            while changed {
                changed = false
                for approval in approvals {
                    let subject = valid[approval.subjectEndpointId]!
                    if store.pins[subject.endpointId] != nil || store.declined.contains(subject.endpointId) { continue }
                    guard let approver = store.pins[approval.approverEndpointId] else { continue }
                    let pinned = (approval.kind == .approve && approver.userId == subject.userId)
                        || (approval.kind == .reset && approver.userId != subject.userId && subject.userId != store.userId && roots.contains(subject.endpointId))
                    guard pinned else { continue }
                    store.pins[subject.endpointId] = TrustPin(
                        userId: subject.userId, x25519Public: subject.x25519Public,
                        via: approval.kind == .approve ? .approved : .reset, by: approval.approverEndpointId
                    )
                    changed = true
                }
            }
            let fresh = Set(store.pins.values.map(\.userId)).filter { $0 != store.userId && !store.members.contains($0) }
            store.members.append(contentsOf: fresh.sorted())
        }

        var rows: [TrustRow] = []
        if store.joined {
            for endpointId in order {
                let install = valid[endpointId]!
                if store.pins[endpointId] != nil || store.declined.contains(endpointId) { continue }
                let kind: TrustRow.Kind?
                if install.userId == store.userId {
                    // Own: a build that asked, or a competing root.
                    kind = install.approvalRequested || roots.contains(endpointId) ? .newInstall : nil
                } else {
                    // A team-mate's: only a new root of a member pinned before.
                    kind = roots.contains(endpointId) && store.members.contains(install.userId) ? .reset : nil
                }
                guard let kind, let code = InstallCode.of(endpointId: endpointId) else { continue }
                rows.append(TrustRow(kind: kind, installId: install.installId, endpointId: endpointId, userId: install.userId, label: install.label, code: code))
            }
            rows.sort { a, b in a.kind == b.kind ? a.endpointId < b.endpointId : a.kind == .newInstall }
        }

        let ownChain = order.contains { endpointId in
            valid[endpointId]!.userId == store.userId
                && approvals.contains { ($0.kind == .root || $0.kind == .approve) && $0.subjectEndpointId == endpointId }
        }
        let status: TrustStatus = store.joined ? .trusted : ownChain ? .waiting : .claimRoot
        return TrustView(store: store, status: status, rows: rows)
    }

    /// The approvals from a root of `userId` down to `target` (`approve`
    /// only, every approver of `userId`), shortest first; nil when none.
    static func chain(
        to target: String, userId: String, approvals: [ServedApproval], roots: Set<String>, userOf: (String) -> String?
    ) -> [String]? {
        var parents: [String: String] = [:]
        let starts = Array(Set(approvals.filter { $0.kind == .root && userOf($0.subjectEndpointId) == userId }.map(\.subjectEndpointId))).sorted()
        var queue = starts
        var seen = Set(starts)
        var head = 0
        while head < queue.count {
            let at = queue[head]
            head += 1
            if at == target {
                var path = [at]
                while let parent = parents[path[0]] { path.insert(parent, at: 0) }
                return path
            }
            for approval in approvals where approval.kind == .approve && approval.approverEndpointId == at && !seen.contains(approval.subjectEndpointId) {
                if userOf(approval.subjectEndpointId) != userId || roots.contains(approval.subjectEndpointId) { continue }
                seen.insert(approval.subjectEndpointId)
                parents[approval.subjectEndpointId] = at
                queue.append(approval.subjectEndpointId)
            }
        }
        return nil
    }

    // ── R1 ────────────────────────────────────────────────────────────────

    public enum KeepVerdict: Equatable, Sendable {
        case keep(record: KeyRecord, secret: Data, treePublicKey: String)
        case refuse(WrapRefusal)
    }

    /// R1: whether this install keeps a served wrap (of ANY epoch). A
    /// signature that fails, a malformed wrap and one that does not open are
    /// refused in every mode; `record` keeps an unsigned wrap (`unsigned`)
    /// and one signed by an UNPINNED install that verifies (`refused-would-be`).
    public static func keepWrap(
        _ wrap: HeldTreeKey, identity: Data, installSecret: Data, store: TrustStore, mode: TrustMode, crypto: TrustCrypto
    ) -> KeepVerdict {
        let pins = Array(store.pins.keys)
        switch crypto.acceptWrap(wrap, identity: identity, installSecret: installSecret, pins: pins, required: mode == .mandatory) {
        case let .verified(secret, key): return .keep(record: .verified, secret: secret, treePublicKey: key)
        case let .unsigned(secret, key): return .keep(record: .unsigned, secret: secret, treePublicKey: key)
        case .refused(.unpinned) where mode == .record:
            // Is it at least its signer's? Then it is kept, and recorded.
            guard let signer = wrap.signerEndpointId else { return .refuse(.signature) }
            switch crypto.acceptWrap(wrap, identity: identity, installSecret: installSecret, pins: [signer], required: false) {
            case let .verified(secret, key): return .keep(record: .refusedWouldBe, secret: secret, treePublicKey: key)
            case let .refused(reason): return .refuse(reason)
            case .unsigned: return .refuse(.signature)
            }
        case let .refused(reason): return .refuse(reason)
        }
    }

    // ── R2 ────────────────────────────────────────────────────────────────

    /// R2: `false` (= `rollback`, every mode) when an answer names an epoch
    /// below the highest one this install verified for the root.
    public static func sealEpochOk(_ store: TrustStore, rootId: String, epoch: UInt32) -> Bool {
        epoch >= (store.roots[rootId.lowercased()]?.verifiedEpoch ?? 0)
    }

    // ── R3 ────────────────────────────────────────────────────────────────

    public enum SealRefusal: String, Sendable { case malformed, unsigned, unpinned, unopened, signature, rollback, keyMismatch }

    public enum SealVerdict: Equatable, Sendable {
        case seal(record: KeyRecord)
        case refuse(SealRefusal)
    }

    /// A signed wrap of that epoch for another install (`acceptVouch`).
    public struct Vouch: Codable, Equatable, Sendable {
        public var recipientEndpointId: String
        public var wrapped: String
        public var signature: String
        public var signerEndpointId: String

        public init(recipientEndpointId: String, wrapped: String, signature: String, signerEndpointId: String) {
            self.recipientEndpointId = recipientEndpointId
            self.wrapped = wrapped
            self.signature = signature
            self.signerEndpointId = signerEndpointId
        }
    }

    private static func sealRefusal(_ refusal: WrapRefusal) -> SealRefusal { SealRefusal(rawValue: refusal.rawValue)! }

    /// R3: whether this install seals to (root, epoch, public key) as the
    /// server names it. With a held secret of that epoch: only to ITS public
    /// key. Without: a vouch by a pinned install = `verified`; none =
    /// `unsigned` and one by an unpinned install that verifies =
    /// `refused-would-be`, both sealed in `record` mode, refused once
    /// mandatory; a vouch whose signature fails is refused in every mode.
    public static func sealTo(
        rootId: String, epoch: UInt32, publicKey: String, held: (secret: Data, record: KeyRecord)?, vouch: Vouch?,
        store: TrustStore, mode: TrustMode, crypto: TrustCrypto
    ) -> SealVerdict {
        guard sealEpochOk(store, rootId: rootId, epoch: epoch) else { return .refuse(.rollback) }
        if let held {
            guard TreePublicKeys.of(secret: held.secret) == publicKey else { return .refuse(.keyMismatch) }
            if mode == .mandatory, held.record == .unsigned { return .refuse(.unsigned) }
            if mode == .mandatory, held.record == .refusedWouldBe { return .refuse(.unpinned) }
            return .seal(record: held.record)
        }
        guard let vouch else { return mode == .record ? .seal(record: .unsigned) : .refuse(.unsigned) }
        let statement = WrapStatement(rootId: rootId, epoch: epoch, treePublicKey: publicKey, recipientEndpointId: vouch.recipientEndpointId, wrapped: vouch.wrapped)
        switch crypto.acceptVouch(statement, signature: vouch.signature, signerEndpointId: vouch.signerEndpointId, pins: Array(store.pins.keys)) {
        case nil: return .seal(record: .verified)
        case .unsigned: return mode == .record ? .seal(record: .unsigned) : .refuse(.unsigned)
        case .unpinned:
            if let own = crypto.acceptVouch(statement, signature: vouch.signature, signerEndpointId: vouch.signerEndpointId, pins: [vouch.signerEndpointId]) {
                return .refuse(sealRefusal(own))
            }
            return mode == .record ? .seal(record: .refusedWouldBe) : .refuse(.unpinned)
        case let refusal?: return .refuse(sealRefusal(refusal))
        }
    }

    // ── R4 ────────────────────────────────────────────────────────────────

    public enum TargetRefusal: String, Sendable { case unapproved, keyMismatch, registration, notTrusted }

    public enum TargetVerdict: Equatable, Sendable {
        case wrap(record: KeyRecord)
        case refuse(TargetRefusal)

        public var wraps: Bool { if case .wrap = self { true } else { false } }
    }

    /// R4: whether this install wraps a key for a target the server names.
    /// Only a joined install wraps for others; a pinned target with its pinned
    /// user and key = `verified`; a target that ASKED (or claims a root) and
    /// is not pinned is refused in every mode (the waiting state); one that
    /// never asked and claims no root is an older build: `legacy`, wrapped in
    /// `record` mode, refused once mandatory. A registration that does not
    /// verify, or a pinned endpoint served with another key, is refused always.
    public static func wrapTarget(
        _ target: ServedInstall, store: TrustStore, mode: TrustMode, roots: Set<String> = [], crypto: TrustCrypto
    ) -> TargetVerdict {
        if target.endpointId == store.selfEndpointId { return .wrap(record: .verified) }
        guard store.joined else { return .refuse(.notTrusted) }
        if let pin = store.pins[target.endpointId] {
            return pin.x25519Public == target.x25519Public && pin.userId == target.userId ? .wrap(record: .verified) : .refuse(.keyMismatch)
        }
        // An older build that registered before #EXP-1950 kept no signature: legacy too.
        if target.registrationSignature != nil, !registrationValid(target, crypto: crypto) { return .refuse(.registration) }
        if target.approvalRequested || roots.contains(target.endpointId) { return .refuse(.unapproved) }
        return mode == .record ? .wrap(record: .legacy) : .refuse(.unapproved)
    }

    // ── a call's key ──────────────────────────────────────────────────────

    public enum CallKeyVerdict: Equatable, Sendable {
        case key(Data, record: KeyRecord)
        case noKey
        case rollback
    }

    /// A call's key (attack 4): the secret of the (root, epoch) `sendTarget`
    /// names, from this install's KEYRING only. `record` uses any held key;
    /// `mandatory` only `verified` and `legacy` ones; else `no_key`.
    public static func callKey(
        rootId: String, epoch: UInt32, held: (secret: Data, record: KeyRecord)?, store: TrustStore, mode: TrustMode
    ) -> CallKeyVerdict {
        guard sealEpochOk(store, rootId: rootId, epoch: epoch) else { return .rollback }
        guard let held else { return .noKey }
        if mode == .mandatory, held.record != .verified, held.record != .legacy { return .noKey }
        return .key(held.secret, record: held.record)
    }
}
