import Foundation

/// #VAPP-230 (eApps M1): a template READS an upstream. A binding source
/// `upstream:<server>/<tool>?<args>` follows one read-only tool of a team
/// MCP server through the server broker (tRPC `apps.upstreamRead {id,
/// source}`), locked ×4 against packages/emcp/fixtures/upstream.json
/// `binding` (TS `@exp/emcp/upstream`, web `exponential-ui-host.tsx`):
/// ok emits the value at the bound path, a refusal or offline emits nothing
/// (the template's default stays), hosts re-read every `cacheSeconds` while
/// bound, and ONE "Connect <server>" banner shows per server answering
/// `upstreamDisconnected`, sorted (`bannerCases`).
public enum AppUpstream {
    /// The binding scheme (`binding.scheme`).
    public static let scheme = "upstream"
    /// The broker's cache and the hosts' re-read period (`binding.cacheSeconds`).
    public static let cacheSeconds = 60
    /// The banner's text, `{server}` filled (`binding.banner`).
    public static let banner = "Connect {server}"
    /// The banner's action (opens Settings › MCP; the web host's button).
    public static let connect = "Connect"
    /// The banner's test id (the web's `exponential-ui-upstream-connect`).
    public static let bannerTestId = "exponential-ui-upstream-connect"
    /// The upstream error that raises the banner (`errors.disconnected`).
    public static let disconnected = "upstreamDisconnected"

    /// One read's answer (`apps.upstreamRead`): the value, or the refusal's
    /// error and the server it named.
    public enum Answer: Equatable, Sendable {
        case ok(CardJSON)
        case refused(error: String, server: String)
        /// #VAPP-239: a device-routed server whose device is offline
        /// (`{ok: false, error: upstreamFailed, server, offline: <device>}`).
        case offline(server: String, device: String)
    }

    /// #VAPP-239: an offline device's banner (`device.copy.offline`).
    public static let offlineBanner = "{server} runs on {device}, offline"

    /// One host banner: its text and whether it carries the Connect action.
    public struct HostBanner: Equatable, Sendable, Identifiable {
        public let server: String
        public let text: String
        public let connect: Bool
        public var id: String { server }
    }

    /// #VAPP-239 (`device.bannerCases`): `Connect <server>` for an
    /// upstreamDisconnected read, the offline copy (no action) for an offline
    /// device; one per server, sorted by server.
    public static func hostBanners(_ answers: [Answer]) -> [HostBanner] {
        var byServer: [String: HostBanner] = [:]
        for answer in answers {
            switch answer {
            case let .refused(error, server) where error == disconnected && byServer[server] == nil:
                byServer[server] = HostBanner(server: server, text: bannerText(server), connect: true)
            case let .offline(server, device) where byServer[server] == nil:
                byServer[server] = HostBanner(
                    server: server,
                    text: offlineBanner.replacingOccurrences(of: "{server}", with: server).replacingOccurrences(of: "{device}", with: device),
                    connect: false
                )
            default:
                break
            }
        }
        return byServer.keys.sorted().compactMap { byServer[$0] }
    }

    /// The servers the viewer must connect: every server whose read
    /// answered `upstreamDisconnected`, each once, sorted.
    public static func bannerServers(_ answers: [Answer]) -> [String] {
        var servers = Set<String>()
        for answer in answers {
            if case let .refused(error, server) = answer, error == disconnected { servers.insert(server) }
        }
        return servers.sorted()
    }

    /// The banners' texts, one per `bannerServers` entry, in its order.
    public static func banners(_ answers: [Answer]) -> [String] {
        bannerServers(answers).map(bannerText)
    }

    /// `banner` for one server.
    public static func bannerText(_ server: String) -> String {
        banner.replacingOccurrences(of: "{server}", with: server)
    }
}
