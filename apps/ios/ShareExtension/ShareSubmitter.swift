import Foundation

/// Runs the create → upload → patch sequence against the API, mirroring the
/// app's `CreateIssueSheet`. Images are uploaded sequentially (extension memory
/// is tight) and embedded into the description only after the issue exists — the
/// create mutation rejects markdown images.
@MainActor
struct ShareSubmitter {
    let issuesApi: IssuesApi
    let issueImagesApi: IssueImagesApi

    func submit(payload: SharedPayload, accountId: String, projectId: String) async throws {
        let base = payload.descriptionText.trimmingCharacters(in: .whitespacesAndNewlines)
        let titleText = payload.title.trimmingCharacters(in: .whitespacesAndNewlines)

        let createdId = try await issuesApi.create(
            accountId: accountId,
            CreateIssueInput(
                projectId: projectId,
                title: titleText.isEmpty ? "Shared" : titleText,
                description: base.isEmpty ? nil : base
            )
        )

        var imageMarkdown = ""
        for image in payload.images {
            let uploaded = try await issueImagesApi.upload(
                accountId: accountId,
                issueId: createdId,
                data: image.data,
                filename: image.filename,
                contentType: image.contentType
            )
            // uploaded.url is the canonical relative form /api/attachments/{id}.
            imageMarkdown += "\n\n![](\(uploaded.url))"
        }

        if !imageMarkdown.isEmpty {
            let finalText = (base + imageMarkdown).trimmingCharacters(in: .whitespacesAndNewlines)
            try await issuesApi.update(
                accountId: accountId,
                UpdateIssueInput(id: createdId, description: finalText)
            )
        }

        SharedProjectMirror.writeLastUsed(accountId: accountId, projectId: projectId)
    }
}
