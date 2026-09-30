import ExpCore
import ExpUI
import SwiftUI

/// EXP-1003 — the in-app preview for a markdown attachment (web EXP-955's
/// `attachment-markdown-preview.tsx`): the file renders read-only through the
/// same cmark-gfm block stack comments use, with the neutral interchange
/// palette, instead of going to Quick Look as plain text.
///
/// The size is checked TWICE (`AttachmentFiles.markdownPreviewSkipsFetch` /
/// `markdownPreviewOutcome`): a synced `size_bytes` over 1 MiB opens straight
/// into the download hint without a request, and the fetched text is measured
/// again for legacy rows that carry `size_bytes = 0`. "Download" writes the
/// bytes to the same temp layout the Files rail uses and hands them to the
/// share sheet, whose "Save to Files" is the iOS download.
struct AttachmentMarkdownPreviewSheet: View {
    let attachment: AttachmentEntity

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId

    private enum Phase {
        case loading
        case ready(String)
        case tooLarge
        case error(String)
    }

    @State private var phase: Phase = .loading
    /// The fetched bytes, kept so Download writes them without a second request.
    @State private var data: Data?
    @State private var downloading = false
    /// A failed Download keeps the rendered file on screen; the reason shows
    /// under the subtitle instead.
    @State private var downloadError: String?
    @State private var shareTarget: ShareTarget?

    private var title: String {
        attachment.filename.isEmpty ? "Preview" : attachment.filename
    }

    private var markdownContext: AgentMarkdownContext {
        AgentMarkdownContext(
            baseURL: deps.auth.instanceBaseURL(forAccountId: accountId),
            accountId: accountId,
            httpClient: deps.httpClient,
            // `#IDENT` refs chip to same-team issues like on desktop/Android
            // (and the web dialog); a tap navigates out of the sheet. A bare
            // `EXP-12` in a file stays text, as on the web.
            issueRefs: AgentIssueRefContext(
                teamId: attachment.teamId,
                db: deps.db,
                onOpen: { issueId in
                    deps.deepLinkBus.navigateToIssue(issueId, accountId: accountId)
                },
                bareRefs: false
            )
        )
    }

    var body: some View {
        GlassSheetChrome(
            title: title,
            height: .full,
            pinnedHeader: {
                VStack(alignment: .leading, spacing: 4) {
                    Text(AttachmentFiles.markdownPreviewSubtitle(sizeBytes: attachment.sizeBytes))
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    if let downloadError {
                        Text(downloadError)
                            .font(.caption)
                            .foregroundStyle(DesignTokens.Semantic.red)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, GlassSheetTokens.headerHPadding)
                .padding(.bottom, 8)
            },
            content: {
                ScrollView {
                    phaseView
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.horizontal, GlassSheetTokens.headerHPadding)
                        .padding(.bottom, 16)
                }
            },
            primaryAction: {
                GlassSubmitButton("Download", loading: downloading) {
                    download()
                }
            }
        )
        .accessibilityIdentifier("attachment-markdown-preview")
        .task(id: attachment.id) { await load() }
        .sheet(item: $shareTarget) { target in
            ActivityShareSheet(items: [target.url])
        }
    }

    @ViewBuilder
    private var phaseView: some View {
        switch phase {
        case .loading:
            HStack(spacing: 6) {
                ProgressView()
                    .controlSize(.small)
                    .tint(.white)
                Text("Loading...")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            .padding(.vertical, 24)
        case let .ready(markdown):
            AgentMarkdownText(
                text: markdown,
                context: markdownContext,
                overrides: MarkdownStyle.Overrides()
            )
        case .tooLarge:
            Text("This file is too large to preview here. Download it to read it.")
                .font(.callout)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .padding(.vertical, 24)
        case let .error(message):
            Text(message)
                .font(.callout)
                .foregroundStyle(DesignTokens.Semantic.red)
                .padding(.vertical, 24)
        }
    }

    // MARK: - Actions

    private func load() async {
        if AttachmentFiles.markdownPreviewSkipsFetch(sizeBytes: attachment.sizeBytes) {
            phase = .tooLarge
            return
        }
        phase = .loading
        do {
            let bytes = try await deps.attachmentsApi.download(
                accountId: accountId,
                relativeUrl: attachment.url
            )
            data = bytes
            switch AttachmentFiles.markdownPreviewOutcome(data: bytes) {
            case let .ready(text): phase = .ready(text)
            case .tooLarge: phase = .tooLarge
            }
        } catch is CancellationError {
            return
        } catch {
            phase = .error(AttachmentFiles.markdownPreviewErrorMessage(error))
        }
    }

    /// Same temp layout as `IssueDetailViewModel.downloadForPreview`: one
    /// folder per attachment, reused on a second tap.
    private func download() {
        guard !downloading else { return }
        downloading = true
        downloadError = nil
        Task {
            defer { downloading = false }
            let directory = FileManager.default.temporaryDirectory
                .appendingPathComponent("attachments", isDirectory: true)
                .appendingPathComponent(attachment.id, isDirectory: true)
            let destination = directory
                .appendingPathComponent(AttachmentFiles.sanitizedFilename(attachment.filename))
            do {
                if !FileManager.default.fileExists(atPath: destination.path) {
                    let bytes: Data
                    if let data {
                        bytes = data
                    } else {
                        bytes = try await deps.attachmentsApi.download(
                            accountId: accountId,
                            relativeUrl: attachment.url
                        )
                    }
                    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
                    try bytes.write(to: destination, options: .atomic)
                }
                shareTarget = ShareTarget(url: destination, text: attachment.filename)
            } catch {
                downloadError = AttachmentFiles.markdownPreviewErrorMessage(error)
            }
        }
    }
}
