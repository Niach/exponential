import Foundation

private struct TeamImageUploadResult: Decodable {
    let iconUrl: String
}

// Its own file: the share extension compiles `AttachmentsApi.swift` without the
// rest of ExpCore (`TeamImage` included).
extension AttachmentsApi {
    /// #EXP-1645 (team-image.json `upload`): the team image. Same multipart
    /// part as every upload; the owner-only route normalises it to one PNG and
    /// answers the `teams.icon_url` it set (the shape delivers the row too).
    public func uploadTeamImage(
        accountId: String,
        teamId: String,
        data: Data,
        filename: String,
        contentType: String
    ) async throws -> String {
        let responseData = try await post(
            accountId: accountId,
            path: TeamImage.uploadPath(teamId: teamId),
            data: data,
            filename: filename,
            contentType: contentType,
            media: nil
        )
        return try JSONDecoder().decode(TeamImageUploadResult.self, from: responseData).iconUrl
    }
}
