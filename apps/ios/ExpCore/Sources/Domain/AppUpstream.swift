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
