import ExpUI
import ExpCore
import QuickLook
import SwiftUI

/// The issue's non-image attachments (EXP-297).
///
/// Files are NOT part of the description markdown — they render straight from
/// the synced `attachments` rows, so nothing about the editor or the inline
/// image pipeline is involved here. Tapping a row downloads the bytes into a
/// temp folder and hands them to Quick Look, whose own share button covers
/// "save"/"open in…" — so there is no bespoke export UI (and, per EXP-297, no
/// video player anywhere).
///
/// EXP-327: there is no attach button here any more, and no empty state. Files
/// are attached from the description editor's image button ("Files / Photo
/// library"), which is the one place a user reaches for when adding something —
/// so with nothing attached this section renders nothing at all.
struct IssueFilesSection: View {
    let viewModel: IssueDetailViewModel

    @State private var previewURL: URL?
    @State private var downloadingId: String?
    @State private var pendingDelete: AttachmentEntity?

    private var canManage: Bool { viewModel.canManageFiles }

    private var isEmpty: Bool {
        viewModel.fileAttachments.isEmpty && viewModel.pendingFileUploads.isEmpty
    }

    var body: some View {
        // No files (and none in flight): stay out of the way entirely. A failed
        // upload keeps a pending row, so errors still have somewhere to surface.
        Group {
            if !isEmpty {
                content
            }
        }
    }

    private var content: some View {
        VStack(alignment: .leading, spacing: 8) {
            header
            VStack(spacing: 6) {
                ForEach(viewModel.fileAttachments) { attachment in
                    attachmentRow(attachment)
                }
                ForEach(viewModel.pendingFileUploads) { pending in
                    pendingRow(pending)
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .quickLookPreview($previewURL)
        .confirmationDialog(
            "Delete file?",
            isPresented: Binding(
                get: { pendingDelete != nil },
                set: { if !$0 { pendingDelete = nil } }
            ),
            presenting: pendingDelete
        ) { attachment in
            Button("Delete", role: .destructive) {
                Task { await viewModel.deleteAttachment(id: attachment.id) }
            }
            Button("Cancel", role: .cancel) {}
        } message: { attachment in
            Text("\(attachment.filename) will be permanently deleted.")
        }
    }

    private var header: some View {
        HStack {
            Text("Files")
                .font(.subheadline.weight(.medium))
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .accessibilityIdentifier("issue-files-header")
            Spacer()
        }
    }

    // MARK: - Rows

    private func attachmentRow(_ attachment: AttachmentEntity) -> some View {
        let isDownloading = downloadingId == attachment.id
        let isDeleting = viewModel.deletingAttachmentIds.contains(attachment.id)
        // EXP-603: the row is a tap target rather than a `Button` so the
        // actions menu (which replaced a long-press `.contextMenu`) can be a
        // button of its own inside it. Tapping the row still previews.
        return row(
            symbol: AttachmentFiles.sfSymbolName(forContentType: attachment.contentType),
            title: attachment.filename,
            subtitle: formatSize(attachment.sizeBytes)
        ) {
            if isDownloading || isDeleting {
                ProgressView()
                    .controlSize(.small)
                    .tint(.white)
            } else {
                AppIcon(AppIcons.uiDownload, size: AppIcon.Size.small)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            GlassMenu {
                GlassMenuItem("Preview", icon: AppIcons.uiWatch) {
                    preview(attachment)
                }
                if canManage {
                    GlassMenuItem("Delete", icon: AppIcons.uiDelete, destructive: true) {
                        pendingDelete = attachment
                    }
                }
            } label: {
                AppIcon(AppIcons.uiMore, size: AppIcon.Size.small)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .padding(6)
            }
            .accessibilityLabel("File actions")
        }
        .onTapGesture {
            guard !isDeleting else { return }
            preview(attachment)
        }
    }

    private func pendingRow(_ pending: PendingFileUpload) -> some View {
        row(
            symbol: AttachmentFiles.sfSymbolName(forContentType: pending.contentType),
            title: pending.filename,
            subtitle: pending.failure ?? formatSize(pending.sizeBytes),
            subtitleTint: pending.failure == nil
                ? .white.opacity(TextOpacity.tertiary)
                : DesignTokens.Semantic.red
        ) {
            if pending.failure == nil {
                ProgressView()
                    .controlSize(.small)
                    .tint(.white)
            } else {
                HStack(spacing: 6) {
                    Button("Retry") {
                        viewModel.retryUpload(pendingId: pending.id)
                    }
                    .font(.caption)
                    .buttonStyle(.plain)
                    .foregroundStyle(.white)
                    Button("Dismiss") {
                        viewModel.cancelPendingUpload(pendingId: pending.id)
                    }
                    .font(.caption)
                    .buttonStyle(.plain)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
            }
        }
    }

    private func row(
        symbol: String,
        title: String,
        subtitle: String,
        subtitleTint: Color = .white.opacity(TextOpacity.tertiary),
        @ViewBuilder trailing: () -> some View
    ) -> some View {
        HStack(spacing: 10) {
            Image(systemName: symbol)
                .font(.system(size: 15))
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .frame(width: 20)
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .font(.callout)
                    .foregroundStyle(.white)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Text(subtitle)
                    .font(.caption2)
                    .foregroundStyle(subtitleTint)
                    .lineLimit(2)
            }
            Spacer(minLength: 8)
            trailing()
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .glassRow()
        .contentShape(Rectangle())
    }

    // MARK: - Actions

    private func preview(_ attachment: AttachmentEntity) {
        guard downloadingId == nil else { return }
        downloadingId = attachment.id
        Task {
            let url = await viewModel.downloadForPreview(attachment)
            downloadingId = nil
            if let url { previewURL = url }
        }
    }

    private func formatSize(_ bytes: Int) -> String {
        Int64(bytes).formatted(.byteCount(style: .file))
    }
}
