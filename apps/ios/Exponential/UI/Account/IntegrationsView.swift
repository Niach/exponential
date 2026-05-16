import SwiftUI

struct IntegrationsView: View {
    @Environment(AppDependencies.self) private var deps
    @State private var googleConnected = false
    @State private var googleConnectedAt: String?
    @State private var loading = true
    @State private var error: String?
    @State private var backfilling = false

    var body: some View {
        ZStack {
            AppBackground()

            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    // Google Calendar card
                    VStack(alignment: .leading, spacing: 12) {
                        HStack(spacing: 10) {
                            Image(systemName: "calendar")
                                .font(.title2)
                                .foregroundStyle(.white)

                            VStack(alignment: .leading, spacing: 2) {
                                Text("Google Calendar")
                                    .font(.body.weight(.medium))
                                    .foregroundStyle(.white)

                                Text("Sync issue due dates as calendar events")
                                    .font(.caption)
                                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            }
                        }

                        if loading {
                            ProgressView().tint(.white)
                        } else if googleConnected {
                            HStack(spacing: 6) {
                                Circle()
                                    .fill(.green)
                                    .frame(width: 8, height: 8)
                                Text("Connected")
                                    .font(.caption)
                                    .foregroundStyle(.green)
                                if let at = googleConnectedAt {
                                    Text("since \(at.prefix(10))")
                                        .font(.caption)
                                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                }
                            }

                            HStack(spacing: 10) {
                                Button {
                                    Task { await backfill() }
                                } label: {
                                    HStack(spacing: 4) {
                                        if backfilling {
                                            ProgressView().tint(.white)
                                        }
                                        Text("Backfill")
                                    }
                                    .font(.subheadline)
                                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                                    .padding(.horizontal, 14)
                                    .padding(.vertical, 8)
                                }
                                .glassButton()
                                .disabled(backfilling)
                                .buttonStyle(.plain)

                                Button {
                                    Task { await disconnect() }
                                } label: {
                                    Text("Disconnect")
                                        .font(.subheadline)
                                        .foregroundStyle(.red.opacity(0.8))
                                        .padding(.horizontal, 14)
                                        .padding(.vertical, 8)
                                }
                                .glassButton()
                                .buttonStyle(.plain)
                            }
                        } else {
                            Text("Not connected")
                                .font(.caption)
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        }

                        if let error {
                            Text(error)
                                .font(.caption)
                                .foregroundStyle(.red)
                        }
                    }
                    .padding(16)
                    .glassSection()
                }
                .padding(16)
            }
        }
        .navigationTitle("Integrations")
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .task { await loadStatus() }
    }

    private func loadStatus() async {
        do {
            let status = try await deps.integrationsApi.googleStatus()
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
            try await deps.integrationsApi.googleDisconnect()
            googleConnected = false
        } catch {
            self.error = error.localizedDescription
        }
    }

    private func backfill() async {
        backfilling = true
        do {
            try await deps.integrationsApi.googleBackfill()
        } catch {
            self.error = error.localizedDescription
        }
        backfilling = false
    }
}
