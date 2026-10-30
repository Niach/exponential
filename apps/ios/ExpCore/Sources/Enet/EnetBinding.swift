import Foundation

// VAPP-40: the FULL eNet node (exponential-net-ffi) linked into ExpCore.
// Project.swift compiles the generated binding into this module and sets
// `ENET_NODE` only when `out/ExponentialNetFFI.xcframework` was built
// (`bash apps/desktop/crates/exponential-net-ffi/build-ios.sh`); without it
// the app builds and runs with eNet off. The Notification Service Extension
// keeps the push-only build (ExpPush).

#if ENET_NODE
/// The binding's crypto, for `InstallNodeService`.
public struct EnetInstallCrypto: InstallCrypto {
    public init() {}

    public func endpointId(identity: Data) throws -> String {
        try endpointIdOf(identity: identity)
    }

    public func x25519Public(identity: Data) throws -> Data {
        try pushKeypairDerive(identity: identity).public
    }

    public func sign(identity: Data, message: Data) throws -> Data {
        try identitySign(identity: identity, message: message)
    }
}

/// An `EnetNode` as an `InstallNodeDriver`.
public final class EnetInstallNode: InstallNodeDriver {
    public let node: EnetNode
    public let endpointId: String

    public init(node: EnetNode) {
        self.node = node
        endpointId = node.endpointId()
    }

    /// Our relay (https://relay.exponential.at), n0 relays off, nobody allowed.
    public static func start(identity: Data) async throws -> EnetInstallNode {
        try await start(config: defaultConfig(), identity: identity)
    }

    /// Relays off, bound to loopback: two nodes on one machine (the tests).
    public static func localOnlyConfig() -> EnetConfig {
        var config = defaultConfig()
        config.relaysEnabled = false
        config.bindAddr = "127.0.0.1:0"
        return config
    }

    public static func start(config: EnetConfig, identity: Data) async throws -> EnetInstallNode {
        EnetInstallNode(node: try await EnetNode.start(config: config, identity: identity))
    }

    public func allowedPeers() -> [String] { node.allowedPeers() }
    public func allow(peer: String) throws { try node.allowPeer(peer: peer) }
    public func revoke(peer: String) throws { try node.revokePeer(peer: peer) }
    public func shutdown() async { try? await node.shutdown() }

    public func sendChat(peer: String, body: Data) async throws {
        _ = try await node.sendChat(peer: peer, body: body)
    }

    public func nextEvent() async -> InstallNodeEvent? {
        switch await node.nextEvent() {
        case nil: return nil
        case let .chat(from, message)?: return .chat(from: from, body: message.body)
        case let .peerRefused(peer, _)?: return .peerRefused(peer: peer)
        case _?: return .other
        }
    }
}
#endif

extension InstallNodeCoordinator {
    /// The app's install node over the real binding, or nil when this build
    /// does not link eNet.
    public static func make(
        trpc: TrpcClient,
        auth: AuthRepository,
        db: DatabaseManager,
        identity: InstallIdentitySource,
        label: String?,
        platform: String?,
        onIdentityRotated: @escaping @Sendable () -> Void
    ) -> InstallNodeCoordinator? {
        #if ENET_NODE
        let service = InstallNodeService(
            registry: InstallsApi(trpc: trpc),
            crypto: EnetInstallCrypto(),
            identity: identity,
            label: label,
            platform: platform,
            onIdentityRotated: onIdentityRotated,
            startNode: { try await EnetInstallNode.start(identity: $0) }
        )
        return InstallNodeCoordinator(
            service: service, auth: auth, db: db,
            steer: InstallSteerCarrier(service: service, lookup: InstallsApi(trpc: trpc))
        )
        #else
        return nil
        #endif
    }
}
