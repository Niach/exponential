import SwiftUI
import ExponentialUI

struct ContentView: View {
    // `express` (a launch argument) or the button: the Express stream.
    @State private var express = ProcessInfo.processInfo.arguments.contains("express")

    var body: some View {
        if express {
            ExpressStream()
        } else {
            Agent(express: $express)
        }
    }
}

struct Agent: View {
    @Binding var express: Bool
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
            Button("Express stream") { express = true }
        }
        .task { host.connect() }
    }
}
