import Foundation

/// Release 2026-10-10 (F6): non-image FILE attachments on a start prompt or
/// a steer message (`[name](/api/attachments/<id>)` lines) need a host that
/// localizes them, and such a host advertises the `steer-files` device cap
/// (desktop IDE + CLI daemon 0.14.66 and newer). The server refuses a start
/// carrying a file for a device without it, so the composers ×4 never offer
/// one: the "Add file or image" pick accepts IMAGES only there and says why
/// with the server's own sentence. A row with NO caps at all (a very old
/// device) lacks the cap too.
public enum SteerFiles {
    /// The device cap (contract `codingSession.steerFilesCap`, desktop
    /// `ACTION_CAPS`).
    public static let cap = DomainContract.codingSessionSteerFilesCap
    /// The ONE sentence ×4: the server's PRECONDITION_FAILED message
    /// (contract `composerUi.filesNeedNewerDevice`).
    public static let needsNewerDeviceNotice = DomainContract.composerUiFilesNeedNewerDevice

    /// Whether the chosen device takes non-image files at all.
    public static func accepted(caps: [String]?) -> Bool {
        caps?.contains(cap) == true
    }

    /// Whether ONE pick may be queued for the device: an inline image always,
    /// anything else only with the cap.
    public static func accepts(contentType: String, caps: [String]?) -> Bool {
        AttachmentFiles.isInlineImage(contentType: contentType) || accepted(caps: caps)
    }
}
