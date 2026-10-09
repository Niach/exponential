// VAPP-91 sample: a blank iOS app hosting an Exponential UI surface streamed
// from the local A2UI JSONL server (samples/exponential-ui/server), with the
// server's custom theme and ONE custom extension component (TrendLine,
// painted natively below). No Exponential account, no backend of ours: the
// SDK's public API only (`ExponentialHost` + a transport + `HostSurface`).
//
// Launch arguments: `-live` keeps the stream open (a reading every 3 s;
// default `?once=1`, the surface only), `-mode dark`, `-server <url>`.
import SwiftUI
import ExponentialUI

enum Config {
    static let arguments = ProcessInfo.processInfo.arguments
    static func value(_ flag: String) -> String? {
        arguments.firstIndex(of: flag).flatMap { arguments.indices.contains($0 + 1) ? arguments[$0 + 1] : nil }
    }

    static let server = URL(string: value("-server") ?? "http://localhost:4190")!
    static let live = arguments.contains("-live")
    static let mode: Mode = value("-mode") == "dark" ? .dark : .light
}

@main
struct GreenhouseApp: App {
    var body: some Scene {
        WindowGroup {
            ContentView()
        }
    }
}

/// Fetches the theme + the extension catalog, then builds the host.
@MainActor
@Observable
final class Sample {
    var host: ExponentialHost?
    var theme: ThemeHandle?
    var failure: String?

    func load() async {
        do {
            let (ext, _) = try await URLSession.shared.data(from: Config.server.appending(path: "extension.json"))
            let (themeJSON, _) = try await URLSession.shared.data(from: Config.server.appending(path: "theme.json"))
            let theme = try ThemeHandle.load(json: String(decoding: themeJSON, as: UTF8.self))
            var stream = URLComponents(url: Config.server.appending(path: "a2ui.jsonl"), resolvingAgainstBaseURL: false)!
            if !Config.live { stream.queryItems = [URLQueryItem(name: "once", value: "1")] }
            let host = ExponentialHost(HostOptions(
                transport: JSONLStreamTransport(url: stream.url!, postUrl: Config.server.appending(path: "action"), reconnect: Config.live ? .seconds(2) : nil),
                extensions: [HostExtension(json: String(decoding: ext, as: UTF8.self), painters: ["TrendLine": TrendLinePainter()])],
                policy: HostPolicy(openUrl: { UIApplication.shared.open($0) }),
                theme: theme,
                mode: Config.mode
            ))
            self.theme = theme
            self.host = host
            host.connect()
        } catch {
            failure = "\(Config.server.absoluteString): \(error.localizedDescription)"
        }
    }
}

struct ContentView: View {
    @State private var sample = Sample()

    var body: some View {
        let background = sample.theme?.color("background", mode: Config.mode) ?? Color(.systemBackground)
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                if let host = sample.host {
                    HostSurface(host: host, surfaceId: "greenhouse") {
                        Text("Waiting for the surface…").foregroundStyle(.secondary)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    Text("transport: \(host.status.rawValue)")
                        .font(.system(size: 12, design: .monospaced))
                        .foregroundStyle(.secondary)
                } else if let failure = sample.failure {
                    Text("Could not reach the sample server.\n\(failure)").foregroundStyle(.red)
                } else {
                    Text("Loading the theme and the extension from \(Config.server.absoluteString)…").foregroundStyle(.secondary)
                }
            }
            .frame(maxWidth: 480, alignment: .leading)
            .padding(16)
            .frame(maxWidth: .infinity)
        }
        .background(background.ignoresSafeArea())
        .preferredColorScheme(Config.mode == .dark ? .dark : .light)
        .task { await sample.load() }
    }
}
