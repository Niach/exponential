import ExpCore
import Foundation

/// Files a preview-feedback issue as the LOGGED-IN developer (authenticated-
/// direct path — no expw_ widget key, no synthetic user). The server rejects
/// images on create, so the sequence is:
///   1. issues.create  { projectId, title, description:<text only>, status:"backlog" }
///   2. POST /api/issues/{id}/images  (the flattened annotated screenshot)
///   3. issues.update   description + "\n\n![screenshot](/api/attachments/{id})"
///
/// Target project = mirror.feedbackProjectId ?? previewedProjectId; the
/// workspace is derived server-side. Graceful degradation: if the upload (or the
/// description update) fails, the text issue is kept and the caller is told the
/// screenshot didn't attach.
@MainActor
final class MacFeedbackReporter {
    private let issuesApi: IssuesApi
    private let imagesApi: IssueImagesApi

    init(issuesApi: IssuesApi, imagesApi: IssueImagesApi) {
        self.issuesApi = issuesApi
        self.imagesApi = imagesApi
    }

    /// Result of a file: the new issue id + whether the screenshot made it in,
    /// so the UI can toast "filed FOO-12" vs. "filed, but the screenshot failed."
    struct Filed: Sendable {
        let issueId: String
        let screenshotAttached: Bool
    }

    struct Screenshot: Sendable {
        let data: Data
        let filename: String
        let contentType: String
    }

    /// Resolve the routing target: the mirror's feedbackProjectId when set,
    /// else the project currently being previewed.
    static func targetProjectId(
        mirror: ProjectPreviewMirror?,
        previewedProjectId: String
    ) -> String {
        mirror?.feedbackProjectId ?? previewedProjectId
    }

    func file(
        accountId: String,
        projectId: String,
        title: String,
        description: String,
        screenshot: Screenshot?
    ) async throws -> Filed {
        let trimmedTitle = title.trimmingCharacters(in: .whitespacesAndNewlines)
        let baseDescription = description.trimmingCharacters(in: .whitespacesAndNewlines)

        // 1. Create the text-only issue (no images allowed on create).
        let issueId = try await issuesApi.create(
            accountId: accountId,
            CreateIssueInput(
                projectId: projectId,
                title: trimmedTitle.isEmpty ? "Preview feedback" : trimmedTitle,
                status: IssueStatus.backlog.rawValue,
                description: baseDescription.isEmpty ? nil : baseDescription
            )
        )

        guard let screenshot else {
            return Filed(issueId: issueId, screenshotAttached: false)
        }

        // 2. Upload the flattened screenshot, then 3. embed it. Either step can
        //    fail without losing the issue — keep the text report and report the
        //    miss so the caller can surface "screenshot upload failed."
        do {
            let uploaded = try await imagesApi.upload(
                accountId: accountId,
                issueId: issueId,
                data: screenshot.data,
                filename: screenshot.filename,
                contentType: screenshot.contentType
            )
            let imageMarkdown = "![screenshot](\(uploaded.url))"
            let nextDescription = baseDescription.isEmpty
                ? imageMarkdown
                : "\(baseDescription)\n\n\(imageMarkdown)"
            try await issuesApi.update(
                accountId: accountId,
                UpdateIssueInput(id: issueId, description: nextDescription)
            )
            return Filed(issueId: issueId, screenshotAttached: true)
        } catch {
            return Filed(issueId: issueId, screenshotAttached: false)
        }
    }
}
