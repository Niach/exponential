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
    /// EXP-1635: the raw share, what `ShareTargets.shareChatText` reads.
    var text: String = ""
    var subject: String?
    var url: String?
}
