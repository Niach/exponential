import SwiftUI
import UIKit

struct ContentView: View {
    @ObservedObject var model: PeerModel

    var body: some View {
        NavigationStack {
            Form {
                Section("Relay") {
                    TextField("Relay URL", text: $model.relayURL).textInputAutocapitalization(.never).autocorrectionDisabled()
                    TextField("Viewer ticket", text: $model.ticket).textInputAutocapitalization(.never).autocorrectionDisabled()
                    TextField("Session id", text: $model.sessionId).textInputAutocapitalization(.never).autocorrectionDisabled()
                }
                Section("TURN / STUN") {
                    TextField("host:port", text: $model.turnHost).textInputAutocapitalization(.never).autocorrectionDisabled()
                    TextField("user", text: $model.turnUser).textInputAutocapitalization(.never).autocorrectionDisabled()
                    TextField("pass", text: $model.turnPass).textInputAutocapitalization(.never).autocorrectionDisabled()
                    TextField("realm", text: $model.turnRealm).textInputAutocapitalization(.never).autocorrectionDisabled()
                }
                Section("Run") {
                    Picker("Policy", selection: $model.policy) {
                        ForEach(PeerModel.policies, id: \.self) { Text($0) }
                    }
                    Picker("Scenario", selection: $model.scenario) {
                        ForEach(PeerModel.scenarios, id: \.self) { Text($0) }
                    }
                    LabeledContent("Source", value: model.source)
                    LabeledContent("Core", value: model.coreVersion)
                }
                Section("Actions") {
                    HStack {
                        Button("Connect") { model.connect() }.buttonStyle(.borderedProminent)
                        Button("Bench") { model.bench() }.buttonStyle(.bordered)
                        Button("Reconnect") { model.reconnect(trigger: "manual") }.buttonStyle(.bordered)
                    }
                    HStack {
                        Button("Send results") { model.sendResults() }.buttonStyle(.bordered)
                        Button("Copy JSON") { UIPasteboard.general.string = model.resultJSON }.buttonStyle(.bordered)
                        Button("Close") { model.close() }.buttonStyle(.bordered)
                    }
                }
                Section("State") {
                    LabeledContent("Relay", value: model.relayStatus)
                    LabeledContent("Daemon", value: model.daemonId ?? "-")
                    LabeledContent("Link", value: model.linkState)
                    LabeledContent("Connect", value: model.connectMs.map { String(format: "%.0f ms", $0) } ?? "-")
                    if let r = model.lastReconnect {
                        LabeledContent("Reconnect", value: "\(r.trigger) \(r.ms.map { String(format: "%.0f ms", $0) } ?? "failed")")
                    }
                    if let b = model.benchSummary { Text(b).font(.footnote.monospaced()) }
                }
                Section("Result JSON") {
                    Text(model.resultJSON.isEmpty ? "-" : model.resultJSON).font(.caption2.monospaced()).textSelection(.enabled)
                }
                Section("Diagnostics") {
                    Text(model.diagnostics).font(.caption2.monospaced()).textSelection(.enabled)
                }
                Section("Log") {
                    Text(model.log.suffix(40).joined(separator: "\n")).font(.caption2.monospaced()).textSelection(.enabled)
                }
            }
            .navigationTitle("PeerHello")
        }
    }
}
