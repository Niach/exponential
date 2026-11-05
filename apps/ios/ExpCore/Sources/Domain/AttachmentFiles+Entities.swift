import Foundation

/// The classification over the entity types: apart from `AttachmentFiles.swift`,
/// which the share extension compiles on its own (`SendSealing`, #EXP-1683)
/// without them.
extension AttachmentFiles {
    public static func isFile(_ row: AttachmentEntity) -> Bool {
        isFile(contentType: row.contentType, asFile: row.asFile)
    }

    public static func isFile(_ row: DraftAttachmentDto) -> Bool {
        isFile(contentType: row.contentType, asFile: row.asFile)
    }

    /// `issueFileRows` over the synced rows (issue-files.json).
    public static func issueFileRows(_ rows: [AttachmentEntity], texts: [String?]) -> [AttachmentEntity] {
        issueFileRows(
            rows,
            texts: texts,
            id: \.id,
            contentType: \.contentType,
            asFile: \.asFile,
            commentId: \.commentId
        )
    }
}
