import CryptoKit
import Foundation

// #EXP-1950 (packages/domain-contract/fixtures/wrap-signature.json): every
// tree key wrap this install MAKES carries its signature: the install's eNet
// identity (the keychain's `InstallIdentityStore`, whose public half is
// `installs.endpoint_id`) over (root, epoch, tree public key, recipient's
// endpoint id, wrapped bytes). Signing only: nothing is refused for a missing
// or foreign signature here. The push-only binding (the Notification Service
// Extension, the share extension) never signs: it has no signer.

/// What a wrap signature covers (the binding's `EnetWrapStatement`).
public struct WrapStatement: Equatable, Sendable {
    public let rootId: String
    public let epoch: UInt32
    /// base64url of the epoch's X25519 public key.
    public let treePublicKey: String
    /// The recipient install's endpoint id (lowercase hex), never its row id.
    public let recipientEndpointId: String
    public let wrapped: String

    public init(rootId: String, epoch: UInt32, treePublicKey: String, recipientEndpointId: String, wrapped: String) {
        self.rootId = rootId
        self.epoch = epoch
        self.treePublicKey = treePublicKey
        self.recipientEndpointId = recipientEndpointId
        self.wrapped = wrapped
    }
}

/// The install identity's wrap signatures (the full binding; a fake in tests).
public protocol WrapSigner: Sendable {
    func endpointId(identity: Data) throws -> String
    /// base64url (no padding) of the 64-byte ed25519 signature.
    func sign(identity: Data, statement: WrapStatement) throws -> String
}

public enum TreePublicKeys {
    /// The base64url X25519 public half of a tree secret: a holder signs over
    /// the key of the secret it HOLDS, never the one a server names.
    public static func of(secret: Data) -> String? {
        guard let key = try? Curve25519.KeyAgreement.PrivateKey(rawRepresentation: secret) else { return nil }
        return InstallRegistration.base64Url(key.publicKey.rawRepresentation)
    }
}

#if ENET_NODE
public struct EnetWrapSigner: WrapSigner {
    public init() {}

    public func endpointId(identity: Data) throws -> String {
        try endpointIdOf(identity: identity)
    }

    public func sign(identity: Data, statement: WrapStatement) throws -> String {
        try signWrap(identity: identity, statement: EnetWrapStatement(
            rootId: statement.rootId,
            epoch: statement.epoch,
            treePublicKey: statement.treePublicKey,
            recipientEndpointId: statement.recipientEndpointId,
            wrapped: statement.wrapped
        ))
    }
}
#endif

public enum WrapSignerFactory {
    /// nil without the full binding: every wrap goes unsigned.
    public static func make() -> WrapSigner? {
        #if ENET_NODE
        EnetWrapSigner()
        #else
        nil
        #endif
    }
}

// ── signing again (wrap-signature.json `resign`, decision (c)) ────────────

/// `sessionKeys.resign`: at most this many wraps per call.
public let resignMaxWraps = 200

/// Which check of `sessionKeys.resign` decided a wrap, in the order they apply.
public enum ResignRule: String, Sendable {
    case callerNotReader, staleEpoch, targetNotReader, badSignature, noRow, signedRow, unsignedRow
}

public enum ResignOutcome: String, Sendable { case replaced, skipped, refused }

/// The stored row of the pair a `resign` wrap names.
public enum ResignRow: String, Sendable { case none, unsigned, signed }

/// The server's rule (the TS reference `resignOutcome`): the first rule
/// that applies, and its outcome.
public func resignOutcome(
    callerReader: Bool, currentEpoch: Bool, targetReader: Bool, signatureValid: Bool, row: ResignRow
) -> (rule: ResignRule, outcome: ResignOutcome) {
    let rule: ResignRule = if !callerReader { .callerNotReader }
        else if !currentEpoch { .staleEpoch }
        else if !targetReader { .targetNotReader }
        else if !signatureValid { .badSignature }
        else { switch row { case .none: .noRow; case .signed: .signedRow; case .unsigned: .unsignedRow } }
    let outcome: ResignOutcome = switch rule {
    case .callerNotReader, .targetNotReader, .badSignature: .refused
    case .staleEpoch, .noRow, .signedRow: .skipped
    case .unsignedRow: .replaced
    }
    return (rule, outcome)
}

/// What a holder does after a failed `resign` (`failureCases`): an older
/// server has no such procedure (`notYet`, quietly again on a later pass).
public enum ResignAfter: String, Sendable { case notYet, refused, retry }

public func resignAfter(code: String?) -> ResignAfter {
    switch code {
    case "NOT_FOUND": .notYet
    case "FORBIDDEN", "BAD_REQUEST": .refused
    default: .retry
    }
}
