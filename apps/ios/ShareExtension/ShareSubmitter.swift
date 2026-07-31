import Foundation

/// Runs the create → upload → patch sequence against the API, mirroring the
/// app's `CreateIssueSheet`. Images are uploaded sequentially (extension memory
/// is tight) and embedded into the description only after the issue exists — the
/// create mutation rejects markdown images.
///
/// ONE submitter is retained for the whole compose session, which makes Post
/// resumable: the created issue id and every uploaded attachment URL survive a
/// failure, so a retry continues at the failed step instead of creating a second
/// issue (a deterministic failure such as the 412 storage cap used to add one
/// title-only duplicate per tap). A failed upload never aborts the run either —
/// whatever landed is patched into the description before the error surfaces, so
/// the issue is never left holding attachments it doesn't render.
@MainActor
final class ShareSubmitter {
    let issuesApi: IssuesApi
    let issueImagesApi: IssueImagesApi

    private struct Destination: Equatable {
        let accountId: String
        let boardId: String
    }

    private var destination: Destination?
    private var createdIssueId: String?
    /// Uploaded attachment URLs, keyed by index into `payload.images`.
    private var uploadedUrls: [Int: String] = [:]
    /// The description already written to the server, so a resumed submit skips
    /// a patch that has already landed.
    private var patchedDescription: String?

    init(issuesApi: IssuesApi, issueImagesApi: IssueImagesApi) {
        self.issuesApi = issuesApi
        self.issueImagesApi = issueImagesApi
    }

    func submit(payload: SharedPayload, accountId: String, boardId: String) async throws {
        let target = Destination(accountId: accountId, boardId: boardId)
        if destination != target {
            // A different destination is a new submission, not a retry: the
            // retained issue lives on the board it was created in.
            destination = target
            createdIssueId = nil
            uploadedUrls = [:]
            patchedDescription = nil
        }

        let base = payload.descriptionText.trimmingCharacters(in: .whitespacesAndNewlines)
        let titleText = payload.title.trimmingCharacters(in: .whitespacesAndNewlines)

        let issueId: String
        if let createdIssueId {
            issueId = createdIssueId
        } else {
            issueId = try await issuesApi.create(
                accountId: accountId,
                CreateIssueInput(
                    boardId: boardId,
                    title: titleText.isEmpty ? "Shared" : titleText,
                    description: base.isEmpty ? nil : base
                )
            )
            createdIssueId = issueId
        }

        var firstFailure: Error?
        var failedCount = 0
        for (index, image) in payload.images.enumerated() where uploadedUrls[index] == nil {
            do {
                let uploaded = try await issueImagesApi.upload(
                    accountId: accountId,
                    issueId: issueId,
                    data: image.data,
                    filename: image.filename,
                    contentType: image.contentType
                )
                // uploaded.url is the canonical relative form /api/attachments/{id}.
                uploadedUrls[index] = uploaded.url
            } catch {
                failedCount += 1
                if firstFailure == nil { firstFailure = error }
            }
        }

        // Embed everything that landed, in payload order, BEFORE reporting a
        // failure — an uploaded attachment missing from the description is
        // invisible in the app.
        let imageMarkdown = payload.images.indices
            .compactMap { uploadedUrls[$0] }
            .map { "\n\n![](\($0))" }
            .joined()
        if !imageMarkdown.isEmpty {
            let finalText = (base + imageMarkdown).trimmingCharacters(in: .whitespacesAndNewlines)
            if finalText != patchedDescription {
                try await issuesApi.update(
                    accountId: accountId,
                    UpdateIssueInput(id: issueId, description: finalText)
                )
                patchedDescription = finalText
            }
        }

        if let firstFailure {
            throw ShareSubmitError.imagesFailed(count: failedCount, underlying: firstFailure)
        }

        SharedBoardMirror.writeLastUsed(accountId: accountId, boardId: boardId)
    }
}

/// Partial failure of an otherwise successful submit: the issue exists, only
/// some of its images don't. Posting again resumes at the missing uploads.
enum ShareSubmitError: Error, LocalizedError {
    case imagesFailed(count: Int, underlying: Error)

    var errorDescription: String? {
        switch self {
        case let .imagesFailed(count, underlying):
            let noun = count == 1 ? "image" : "images"
            return "\(underlying.trpcUserMessage) The issue was created. Tap Post to retry \(count) \(noun)."
        }
    }
}
