import Foundation

// EXP-1435: the sealed-push primitives the app and the Notification Service
// Extension share, over eNet's push-only UniFFI binding
// (apps/desktop/crates/exponential-net-ffi/bindings/push/swift). The binding
// links only when its xcframework was built (`ENET_PUSH`, set by Project.swift
// like eUI's `hasExponentialUi`); without it nothing can be opened, the NSE
// delivers the relay's placeholder unchanged and the app registers legacy.

/// An X25519 push keypair: 32-byte secret + 32-byte public.
public struct PushKeypair: Equatable, Sendable {
    public let secret: Data
    public let publicKey: Data

    public init(secret: Data, publicKey: Data) {
        self.secret = secret
        self.publicKey = publicKey
    }

    /// The public key the server's `pushTokens.register` takes: base64url, no
    /// padding.
    public var publicKeyBase64Url: String { Base64Url.encode(publicKey) }
}

public enum PushCrypto {
    /// True when this build links the push binding.
    public static var isAvailable: Bool {
        #if ENET_PUSH
        true
        #else
        false
        #endif
    }

    /// A fresh keypair, or nil when the binding is absent.
    public static func generateKeypair() -> PushKeypair? {
        #if ENET_PUSH
        let pair = pushKeypairGenerate()
        return PushKeypair(secret: pair.secret, publicKey: pair.public)
        #else
        return nil
        #endif
    }

    /// The keypair derived from 32 install identity bytes (eNet's
    /// `PushKeypair::derive`), or nil when the binding is absent or the bytes
    /// are malformed.
    public static func deriveKeypair(identity: Data) -> PushKeypair? {
        #if ENET_PUSH
        guard let pair = try? pushKeypairDerive(identity: identity) else { return nil }
        return PushKeypair(secret: pair.secret, publicKey: pair.public)
        #else
        return nil
        #endif
    }

    /// Opens a sealed payload; nil on ANY failure (bad version, tampering,
    /// wrong key, no binding): a receiver drops such a push silently.
    public static func open(secret: Data, sealed: Data) -> Data? {
        #if ENET_PUSH
        return try? pushOpen(recipientSecret: secret, sealed: sealed)
        #else
        return nil
        #endif
    }
}

/// RFC 4648 §5 base64url, unpadded on encode, padding-tolerant on decode.
public enum Base64Url {
    public static func encode(_ data: Data) -> String {
        data.base64EncodedString()
            .replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: "=", with: "")
    }

    public static func decode(_ string: String) -> Data? {
        var s = string
            .replacingOccurrences(of: "-", with: "+")
            .replacingOccurrences(of: "_", with: "/")
            .replacingOccurrences(of: "=", with: "")
        // A base64 body of length 4n+1 is never valid.
        guard s.count % 4 != 1 else { return nil }
        while s.count % 4 != 0 { s += "=" }
        return Data(base64Encoded: s)
    }
}
