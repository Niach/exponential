import Foundation
import SwiftUI
import ExponentialUI

/// The prose is yours: the SDK hands it to `onText` and draws none of it.
/// Here it goes into two surfaces the app owns (a Markdown each): the run
/// before the block above the agent's surface, the run after it below.
@MainActor
final class ProseSurfaces {
    weak var host: ExponentialHost?
    private var target: String?
    private var run = ""

    func piece(_ piece: String) {
        guard let host else { return }
        if target == nil {
            target = "intro"
            for surfaceId in ["intro", "outro"] {
                host.receive(json(["version": "1.0", "createSurface": ["surfaceId": surfaceId, "catalogId": ExponentialUI.coreCatalog,
                    "components": [["id": "root", "component": "Markdown", "text": ["@path": "/text"]]], "dataModel": ["text": ""]]]))
            }
        }
        // A "\n\n" piece = a block passed: the next run starts.
        if piece == "\n\n" {
            target = "outro"
            run = ""
            return
        }
        run += piece
        host.receive(json(["version": "1.0", "updateDataModel": ["surfaceId": target!, "path": "/text", "value": run]]))
    }

    private func json(_ object: [String: Any]) -> String {
        String(decoding: try! JSONSerialization.data(withJSONObject: object), as: UTF8.self)
    }
}

/// The Express stream: a model's answer written in Express (prose around an
/// `<a2ui>` block); guides/agent `GET /express` replays one.
struct ExpressStream: View {
    @State private var host: ExponentialHost = {
        let prose = ProseSurfaces()
        let host = ExponentialHost(HostOptions(
            // `format: .express`: the agent writes Express; no reconnect, the answer ends.
            transport: SSETransport(
                url: URL(string: "http://localhost:4300/express")!,
                postUrl: URL(string: "http://localhost:4300/action")!,
                reconnect: nil,
                format: .express
            ),
            theme: ThemeHandle.builtin("exponential"),
            mode: .dark,
            onText: { piece in MainActor.assumeIsolated { prose.piece(piece) } }
        ))
        prose.host = host
        return host
    }()

    var body: some View {
        ScrollView {
            VStack(spacing: 16) {
                HostSurface(host: host, surfaceId: "intro")
                HostSurface(host: host, surfaceId: "main")
                HostSurface(host: host, surfaceId: "outro")
            }
            .padding(16)
        }
        .task { host.connect() }
    }
}
