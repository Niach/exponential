import ExpUI
import ExpCore
import SwiftUI
import GRDB

// Surfaces attachments synced via the `attachments` Electric shape as a
// discoverable list. The canonical reference is the markdown embed in the
// description — the upload endpoint at /api/issues/:id/images returns the
// same URL that lives in this row's `url` column.
struct AttachmentListView: View {
    let issueId: String

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var attachments: [AttachmentEntity] = []
    @State private var observationTask: Task<Void, Never>?

    var body: some View {
        if !attachments.isEmpty {
            VStack(alignment: .leading, spacing: 8) {
                Text("Attachments (\(attachments.count))")
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))

                VStack(spacing: 4) {
                    ForEach(attachments, id: \.id) { attachment in
                        AttachmentRow(attachment: attachment)
                    }
                }
            }
            .onAppear { startObserving() }
            .onDisappear { observationTask?.cancel() }
        } else {
            // Still observe — a new upload arrives via Electric and the
            // section appears without a re-render trigger from the caller.
            Color.clear
                .frame(height: 0)
                .onAppear { startObserving() }
                .onDisappear { observationTask?.cancel() }
        }
    }

    private func startObserving() {
        observationTask?.cancel()
        observationTask = Task {
            guard let pool = try? deps.db.pool(forAccountId: accountId) else { return }
            let obs = ValueObservation.tracking { db in
                try AttachmentEntity
                    .filter(Column("issue_id") == issueId)
                    .order(Column("created_at").asc)
                    .fetchAll(db)
            }
            Task {
                for try await rows in obs.values(in: pool) {
                    self.attachments = rows
                }
            }
        }
    }
}

private struct AttachmentRow: View {
    let attachment: AttachmentEntity

    private var isImage: Bool {
        attachment.contentType.hasPrefix("image/")
    }

    var body: some View {
        Button {
            if let url = URL(string: attachment.url) {
                UIApplication.shared.open(url)
            }
        } label: {
            HStack(spacing: 10) {
                Image(systemName: isImage ? "photo" : "paperclip")
                    .font(.body)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .frame(width: 24)

                VStack(alignment: .leading, spacing: 2) {
                    Text(attachment.filename)
                        .font(.subheadline)
                        .foregroundStyle(.white)
                        .lineLimit(1)
                    Text(formatBytes(attachment.sizeBytes))
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }

                Spacer()

                Image(systemName: "arrow.up.right.square")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .glassRow()
        }
        .buttonStyle(.plain)
    }

    private func formatBytes(_ bytes: Int) -> String {
        let f = ByteCountFormatter()
        f.allowedUnits = [.useKB, .useMB, .useGB]
        f.countStyle = .file
        return f.string(fromByteCount: Int64(bytes))
    }
}
