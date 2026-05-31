import Foundation

/// A single image to attach, already normalized to a server-accepted format.
struct SharedImage: Sendable {
    let data: Data
    let filename: String
    let contentType: String
}

/// The content extracted from a share invocation, pre-filled into the compose UI.
struct SharedPayload: Sendable {
    var title: String
    var descriptionText: String
    var images: [SharedImage]
}
