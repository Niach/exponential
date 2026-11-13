#if !SHARE_EXTENSION
import ExpCore
#endif
import Foundation

/// A single image to attach, already normalized to a server-accepted format.
struct SharedImage: Sendable {
    let data: Data
    let filename: String
    let contentType: String
}

/// A non-image file to attach (EXP-1635), any type.
struct SharedFile: Sendable {
    let data: Data
    let filename: String
    let contentType: String
}

/// The content extracted from a share invocation, pre-filled into the compose UI.
struct SharedPayload: Sendable {
    var title: String
    var descriptionText: String
    var images: [SharedImage]
    var files: [SharedFile] = []
    /// Files past the extractor's cap, left unread: the composer's cap note.
    var moreFiles = false
    /// EXP-1635: the raw share, what `ShareTargets.shareChatText` reads.
    var text: String = ""
    var subject: String?
    var url: String?
}

/// A pending attachment of the extension's composer.
struct ShareAttachment: Identifiable, Sendable {
    let id = UUID()
    let data: Data
    let filename: String
    let contentType: String
    let isImage: Bool
}

/// Where a chat send goes.
enum ShareChatDestination: Equatable, Sendable {
    /// The viewer's conversation with an agent (`conversation` nil = none
    /// yet, the send creates it).
    case agent(id: String, teamId: String, conversation: MirroredSealRoot?)
    /// An existing chat (direct share).
    case chat(MirroredSealRoot)
    /// A Temporary session on a device.
    case newChat(teamId: String, deviceId: String, rowIds: [String])
}
