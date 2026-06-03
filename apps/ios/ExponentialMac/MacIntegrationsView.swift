import ExpCore
import ExpUI
import SwiftUI

/// Integrations panel mirroring the iOS `IntegrationsView`, adapted to a macOS
/// sheet. Google Calendar status + Backfill + Disconnect; "Connect" opens the
/// web integrations page (the OAuth link flow stays web-only, matching iOS).
struct MacIntegrationsView: View {
    @Environment(MacAppDependencies.self) private var deps
    @Environment(\.dismiss) private var dismiss
    let accountId: String

    @State private var googleConnected = false
    @State private var googleConnectedAt: String?
    @State private var loading = true
    @State private var error: String?
    @State private var backfilling = false

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text("Integrations").font(.headline)
                Spacer()
                Button("Done") { dismiss() }
            }
            .padding()
            Divider()

            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    HStack(spacing: 10) {
                        Image(systemName: "calendar").font(.title2)
                        VStack(alignment: .leading, spacing: 2) {
                            Text("Google Calendar").font(.body.weight(.medium))
                            Text("Sync issue due dates as calendar events")
                                .font(.caption).foregroundStyle(.secondary)
                        }
                    }

                    if loading {
                        ProgressView()
                    } else if googleConnected {
                        HStack(spacing: 6) {
                            Circle().fill(.green).frame(width: 8, height: 8)
                            Text("Connected").font(.caption).foregroundStyle(.green)
                            if let at = googleConnectedAt {
                                Text("since \(at.prefix(10))").font(.caption).foregroundStyle(.secondary)
                            }
                        }
                        HStack(spacing: 10) {
                            Button { Task { await backfill() } } label: {
                                HStack(spacing: 4) {
                                    if backfilling { ProgressView().controlSize(.small) }
                                    Text("Backfill")
                                }
                            }
                            .disabled(backfilling)

                            Button(role: .destructive) { Task { await disconnect() } } label: {
                                Text("Disconnect")
                            }
                        }
                    } else {
                        Text("Not connected").font(.caption).foregroundStyle(.secondary)
                        Button("Connect…") {
                            if let base = deps.auth.instanceUrl, let url = URL(string: "\(base)/account/integrations") {
                                Platform.open(url)
                            }
                        }
                    }

                    if let error {
                        Text(error).font(.caption).foregroundStyle(.red)
                    }
                }
                .padding(16)
                .glassSection()
                .padding(16)
            }
        }
        .frame(width: 520, height: 360)
        .task { await loadStatus() }
    }

    private func loadStatus() async {
        do {
            let status = try await deps.integrationsApi.googleStatus(accountId: accountId)
            googleConnected = status.connected
            googleConnectedAt = status.connectedAt
            loading = false
        } catch {
            self.error = error.localizedDescription
            loading = false
        }
    }

    private func disconnect() async {
        do {
            try await deps.integrationsApi.googleDisconnect(accountId: accountId)
            googleConnected = false
        } catch {
            self.error = error.localizedDescription
        }
    }

    private func backfill() async {
        backfilling = true
        do {
            try await deps.integrationsApi.googleBackfill(accountId: accountId)
        } catch {
            self.error = error.localizedDescription
        }
        backfilling = false
    }
}
