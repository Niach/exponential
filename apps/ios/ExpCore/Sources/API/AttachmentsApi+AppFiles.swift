import Foundation

// VAPP-221 (eApps R7): app file uploads, kept OUT of AttachmentsApi.swift so
// the Share Extension (which compiles AttachmentsApi.swift on its own, without
// `Apps`) still builds.
extension AttachmentsApi {
    /// VAPP-221 (eApps R7): a file picked for an app's FileUpload.
    /// `POST /api/apps/{appId}/files` (`Apps.appUploadPath`, fixture
    /// `appFiles`) stores it as the app's attachment (write access); the
    /// answer's `url` is the `/api/attachments/{id}` a record field keeps.
    /// Same multipart part (`file`) as every other upload route.
    public func uploadAppFile(
        accountId: String,
        appId: String,
        data: Data,
        filename: String,
        contentType: String
    ) async throws -> UploadedAttachment {
        try await upload(
            accountId: accountId,
            path: Apps.appUploadPath(appId),
            data: data,
            filename: filename,
            contentType: contentType
        )
    }
}
