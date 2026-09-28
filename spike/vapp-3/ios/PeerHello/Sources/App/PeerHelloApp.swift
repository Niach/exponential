import SwiftUI

@main
struct PeerHelloApp: App {
    @StateObject private var model = PeerModel()
    @Environment(\.scenePhase) private var scenePhase

    var body: some Scene {
        WindowGroup {
            ContentView(model: model)
                .onAppear { model.startAutoIfRequested() }
        }
        .onChange(of: scenePhase) { _, phase in
            model.scenePhaseChanged(phase)
        }
    }
}
