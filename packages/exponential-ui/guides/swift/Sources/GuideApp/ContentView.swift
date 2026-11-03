import SwiftUI
import ExponentialUI

struct ContentView: View {
    // One host per app: A2UI messages in over a transport, actions back out.
    @State private var host = ExponentialHost(HostOptions(
        transport: JSONLStreamTransport(
            url: URL(string: "http://localhost:4300/a2ui.jsonl")!,
            postUrl: URL(string: "http://localhost:4300/action")!
        ),
        theme: ThemeHandle.builtin("exponential"),
        mode: .dark
    ))

    var body: some View {
        // The surface the agent creates, as tall as its content: you own the scroller.
        ScrollView {
            HostSurface(host: host, surfaceId: "main") { ProgressView("Waiting for the agent…") }
                .padding(16)
        }
        .task { host.connect() }
    }
}
