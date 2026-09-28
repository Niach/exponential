// VAPP-3 spike (throwaway): compiled to nothing unless the project was generated with
// `TUIST_PEER_SPIKE=1 tuist generate` (apps/ios/Project.swift), which also adds the Rust peer core
// (PeerFFI.xcframework) and its generated bindings to ExpCore. Keeps the core linked so the
// size/cold-start measurement sees the whole thing.
#if PEER_SPIKE
import Foundation

public enum PeerSpike {
    /// Version string of the linked Rust core.
    public static func version() -> String { peerVersion() }

    /// Creates (and closes) a link so the linker keeps str0m, TURN and signing.
    public static func probe(sessionId: String) -> String {
        let config = PeerConfig(
            peerId: "expcore-probe", sessionId: sessionId, stun: nil, turn: nil,
            turnUsername: nil, turnPassword: nil, turnRealm: nil, policy: .all,
            identitySeed: nil, expectedRemotePubkey: nil, tamperSdp: false, tamperSig: false)
        do {
            let link = try PeerLink(config: config)
            defer { link.close() }
            return "\(peerVersion()) \(link.publicKey())"
        } catch {
            return "\(peerVersion()) error: \(error)"
        }
    }
}
#endif
