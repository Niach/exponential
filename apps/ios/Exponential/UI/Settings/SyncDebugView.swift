import ExpCore
import SwiftUI

struct SyncDebugView: View {
    private let debug = SyncDebug.shared

    var body: some View {
        ZStack {
            AppBackground()

            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    shapesSection
                    logSection
                }
                .padding(.horizontal, 16)
                .padding(.top, 8)
                .padding(.bottom, 24)
            }
        }
        .navigationTitle("Sync diagnostics")
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
    }

    private var shapesSection: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Shapes")
                .font(.caption.weight(.medium))
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .padding(.horizontal, 4)

            let sorted = debug.shapes.sorted { $0.key < $1.key }
            if sorted.isEmpty {
                Text("No shape activity yet.")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .padding(.horizontal, 4)
            } else {
                VStack(spacing: 4) {
                    ForEach(sorted, id: \.key) { name, status in
                        shapeRow(name: name, status: status)
                    }
                }
            }
        }
    }

    @ViewBuilder
    private func shapeRow(name: String, status: ShapeStatus) -> some View {
        HStack(spacing: 10) {
            Circle()
                .fill(statusColor(status))
                .frame(width: 8, height: 8)

            Text(name)
                .font(.caption.monospaced())
                .foregroundStyle(.white)
                .lineLimit(1)

            Spacer()

            if status.errorCount > 0 {
                Text("\(status.errorCount) err")
                    .font(.caption2.monospaced())
                    .foregroundStyle(.red.opacity(0.8))
            }

            Text("\(status.requestCount) req")
                .font(.caption2.monospaced())
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))

            Text(status.isLive ? "live" : "init")
                .font(.caption2.weight(.medium))
                .foregroundStyle(status.isLive ? .green.opacity(0.8) : .orange.opacity(0.8))

            Text("HTTP \(status.lastHttpStatus)")
                .font(.caption2.monospaced())
                .foregroundStyle(httpStatusColor(status.lastHttpStatus))
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .glassRow()
    }

    private var logSection: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Log")
                .font(.caption.weight(.medium))
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .padding(.horizontal, 4)

            if debug.lastMessages.isEmpty {
                Text("No log entries yet.")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .padding(.horizontal, 4)
            } else {
                VStack(alignment: .leading, spacing: 2) {
                    ForEach(Array(debug.lastMessages.enumerated()), id: \.offset) { _, message in
                        Text(message)
                            .font(.caption2.monospaced())
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            .lineLimit(1)
                    }
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 8)
                .glassRow()
            }
        }
    }

    private func statusColor(_ status: ShapeStatus) -> Color {
        if status.errorCount > 0 && status.lastHttpStatus != 200 { return .red }
        if status.isLive { return .green }
        return .orange
    }

    private func httpStatusColor(_ code: Int) -> Color {
        if (200...299).contains(code) { return .green.opacity(0.8) }
        if code == 409 { return .orange.opacity(0.8) }
        return .red.opacity(0.8)
    }
}
