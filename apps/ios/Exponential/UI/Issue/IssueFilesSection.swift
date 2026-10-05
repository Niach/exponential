import ExpUI
import ExpCore
import QuickLook
import SwiftUI

/// The issue's FILE attachments (EXP-297): neither inline images nor, since
/// EXP-824, inline video/audio — those two classes live in the description
/// (`![](…)` / `[clip.mp4](…)` blocks rendered by the editor) and a comment's
/// own strip, never here.
///
/// Files are NOT part of the description markdown — they render straight from
/// the synced `attachments` rows, so nothing about the editor or the inline
/// pipelines is involved here. Tapping a row downloads the bytes into a temp
/// folder and hands them to Quick Look, whose own share button covers
/// "save"/"open in…" — so there is no bespoke export UI. EXP-1003: a markdown
/// row (`AttachmentFiles.isMarkdown`) opens the in-app
/// `AttachmentMarkdownPreviewSheet` instead, and its menu keeps the Quick Look
/// route as "Download". Video and audio play
/// inline where they are embedded (`InlineMediaPlayers.swift`), not from this
/// rail.
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
    /// EXP-1003: the markdown row shown in the in-app preview sheet.
    @State private var markdownPreview: AttachmentEntity?

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
        .sheet(item: $markdownPreview) { attachment in
            AttachmentMarkdownPreviewSheet(attachment: attachment)
        }
        // EXP-1215: the app's own alert card (`GlassAlert`), ×4.
        .glassAlert(item: $pendingDelete) { attachment in
            GlassAlert(prompt: Prompts.DeleteFile.copy(filename: attachment.filename), handlers: [
                "delete": { Task { await viewModel.deleteAttachment(id: attachment.id) } },
            ])
        }
    }

    private var header: some View {
        IssueFilesHeader()
    }

    // MARK: - Rows

    private func attachmentRow(_ attachment: AttachmentEntity) -> some View {
        let isDownloading = downloadingId == attachment.id
        let isDeleting = viewModel.deletingAttachmentIds.contains(attachment.id)
        let isMarkdown = AttachmentFiles.isMarkdown(
            contentType: attachment.contentType,
            filename: attachment.filename
        )
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
                AppIcon(isMarkdown ? AppIcons.uiWatch : AppIcons.uiDownload, size: AppIcon.Size.small)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            GlassMenu {
                GlassMenuItem("Preview", icon: AppIcons.uiWatch) {
                    open(attachment, isMarkdown: isMarkdown)
                }
                if isMarkdown {
                    // The Quick Look route, whose share button saves the file.
                    GlassMenuItem("Download", icon: AppIcons.uiDownload) {
                        preview(attachment)
                    }
                }
                if canManage {
                    GlassMenuItem("Delete", icon: AppIcons.uiDelete, destructive: true) {
                        pendingDelete = attachment
                    }
                }
            } label: {
                GhostIconLabel(AppIcons.uiMore)
            }
            .accessibilityLabel("File actions")
        }
        .onTapGesture {
            guard !isDeleting else { return }
            open(attachment, isMarkdown: isMarkdown)
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
        IssueFileRow(
            symbol: symbol,
            title: title,
            subtitle: subtitle,
            subtitleTint: subtitleTint,
            trailing: trailing()
        )
    }

    // MARK: - Actions

    /// EXP-1003: markdown renders in-app; every other file goes to Quick Look.
    private func open(_ attachment: AttachmentEntity, isMarkdown: Bool) {
        if isMarkdown {
            markdownPreview = attachment
        } else {
            preview(attachment)
        }
    }

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

/// The Files section's header — the issue face's and the New issue page's
/// (EXP-1170).
struct IssueFilesHeader: View {
    var body: some View {
        HStack {
            Text("Files")
                .font(.subheadline.weight(.medium))
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .accessibilityIdentifier("issue-files-header")
            Spacer()
        }
    }
}

/// ONE file row — the issue face's attachments and the New issue page's
/// draft files (EXP-1170) draw the same glyph · name · size · trailing.
struct IssueFileRow<Trailing: View>: View {
    let symbol: String
    let title: String
    let subtitle: String
    var subtitleTint: Color = .white.opacity(TextOpacity.tertiary)
    let trailing: Trailing

    var body: some View {
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
            trailing
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .glassRow()
        .contentShape(Rectangle())
    }
}

/// EXP-1170: the New issue page's Files — the draft's attachments (uploaded
/// against the draft, or held in memory for a sub-issue) in the face's rows.
/// Like the face's, it renders nothing while there are none: files are
/// attached from the description editor's attach menu.
struct DraftFilesSection: View {
    let files: [IssueDraftFile]
    let onRemove: (IssueDraftFile) -> Void

    var body: some View {
        if !files.isEmpty {
            VStack(alignment: .leading, spacing: 8) {
                IssueFilesHeader()
                VStack(spacing: 6) {
                    ForEach(files) { file in
                        IssueFileRow(
                            symbol: AttachmentFiles.sfSymbolName(forContentType: file.contentType),
                            title: file.filename,
                            subtitle: Int64(file.sizeBytes).formatted(.byteCount(style: .file)),
                            trailing: GlassMenu {
                                GlassMenuItem("Delete", icon: AppIcons.uiDelete, destructive: true) {
                                    onRemove(file)
                                }
                            } label: {
                                GhostIconLabel(AppIcons.uiMore)
                            }
                            .accessibilityLabel("File actions")
                        )
                    }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}
