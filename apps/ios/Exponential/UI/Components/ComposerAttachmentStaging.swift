#if !SHARE_EXTENSION
import ExpCore
import ExpUI
#endif
import PhotosUI
import SwiftUI
import UIKit

// #EXP-1841 (composer-attachments.json): THE attachment strip of EVERY
// composer — a new chat's (`AgentComposerModel`, "To <agent>"), every
// chat's (`SessionComposerDraft`: a session, an agent chat, the Assistant, an
// app editor) and the share extension's (#EXP-1845, `ShareComposerStrip`).
// One path from a pick (photo picker, camera, Files, paste, the share sheet;
// several at once) to the strip: classify here, stage through
// `ComposerAttachments` (the `[Image #N]` marker at the caret, the caps), toast
// what stayed out; the thumbnail's X drops the entry and renumbers.

/// One picked file, classified (an inline image, any other file, or refused
/// for its size or type).
enum ComposerPick<Attachment> {
    case image(Attachment)
    case file(Attachment)
    case rejected

    var kind: ComposerAttachments.Pick {
        switch self {
        case .image: return .image
        case .file: return .file
        case .rejected: return .rejected
        }
    }
}

/// A composer that holds a plain-text draft and an attachment strip (images
/// first, then files). Compiled into the share extension too.
@MainActor
protocol ComposerAttachmentStaging: AnyObject {
    associatedtype Attachment
    /// The draft and its caret (UTF-16; nil = the end).
    var stagingText: String { get }
    var stagingCaret: Int? { get }
    func setStagingText(_ text: String, caret: Int)
    /// The strip's images, in order (image #N = the N-th).
    var stagedImageIds: [UUID] { get }
    var stagedFileCount: Int { get }
    func appendStaged(_ attachment: Attachment, as kind: ComposerAttachments.Kind)
    func removeStaged(id: UUID)
    /// The strip took a pick (the launch composer's "touched").
    func attachmentsStaged()
    /// What stayed out, as the composer's error toasts.
    func showAttachmentToasts(_ messages: [String])
}

extension ComposerAttachmentStaging {
    func attachmentsStaged() {}

    func showAttachmentToasts(_ messages: [String]) {
        for message in messages {
            Toaster.shared.show(ToastItem(kind: .error, title: message))
        }
    }

    /// The strip as kinds. Numbering and caps only count per kind, so the
    /// images-then-files order the strip draws is the fixture's strip.
    var attachmentStrip: [ComposerAttachments.Kind] {
        Array(repeating: .image, count: stagedImageIds.count)
            + Array(repeating: .file, count: stagedFileCount)
    }

    var imagesFull: Bool { stagedImageIds.count >= ComposerAttachments.maxImages }
    var filesFull: Bool { stagedFileCount >= ComposerAttachments.maxFiles }
    /// Both lists full: nothing more can be attached (the "+" dims).
    var attachFull: Bool { imagesFull && filesFull }

    /// Stage one pick (in pick order): each taken image drops its marker at
    /// the caret, the caret lands behind the last one, and a pick over a cap
    /// or refused toasts.
    func stageAttachments(_ picks: [ComposerPick<Attachment>]) {
        guard !picks.isEmpty else { return }
        let text = stagingText
        let caret = stagingCaret ?? (text as NSString).length
        let staged = ComposerAttachments.stage(
            strip: attachmentStrip, add: picks.map(\.kind), text: text, caret: caret
        )
        for (pick, taken) in zip(picks, staged.taken) where taken {
            switch pick {
            case let .image(picked): appendStaged(picked, as: .image)
            case let .file(picked): appendStaged(picked, as: .file)
            case .rejected: break
            }
        }
        if staged.text != text { setStagingText(staged.text, caret: staged.caret) }
        if staged.taken.contains(true) { attachmentsStaged() }
        if !staged.toasts.isEmpty { showAttachmentToasts(staged.toasts) }
    }

    /// The thumbnail's X: an image's markers leave the draft and the higher
    /// ones slide down; a file's leaves the text alone. The caret stays where
    /// it was, moved left by what left before it.
    func removeAttachment(id: UUID) {
        if let position = stagedImageIds.firstIndex(of: id) {
            let text = stagingText
            let caret = stagingCaret ?? (text as NSString).length
            let dropped = ComposerAttachments.drop(
                strip: attachmentStrip, at: position, text: text, caret: caret
            )
            removeStaged(id: id)
            if dropped.text != text { setStagingText(dropped.text, caret: dropped.caret) }
        } else {
            removeStaged(id: id)
        }
    }
}

#if !SHARE_EXTENSION
/// An app composer: its draft is an `IssueEditorModel`, its strip the
/// steer's pending images and files.
@MainActor
protocol ComposerAttachmentHost: ComposerAttachmentStaging where Attachment == PendingCommentAttachment {
    var draftEditor: IssueEditorModel { get }
    var pendingImages: [PendingSteerImage] { get set }
    var pendingFiles: [PendingSteerFile] { get set }
}

extension ComposerAttachmentHost {
    var stagingText: String { draftEditor.plainText }
    var stagingCaret: Int? { draftEditor.plainTextCaret }
    func setStagingText(_ text: String, caret: Int) { draftEditor.setPlainText(text, caret: caret) }
    var stagedImageIds: [UUID] { pendingImages.map(\.id) }
    var stagedFileCount: Int { pendingFiles.count }

    func appendStaged(_ picked: PendingCommentAttachment, as kind: ComposerAttachments.Kind) {
        switch kind {
        case .image:
            pendingImages.append(PendingSteerImage(
                data: picked.data,
                filename: picked.filename,
                contentType: picked.contentType,
                uploadedId: nil
            ))
        case .file:
            pendingFiles.append(PendingSteerFile(
                data: picked.data, filename: picked.filename, contentType: picked.contentType
            ))
        }
    }

    func removeStaged(id: UUID) {
        pendingImages.removeAll { $0.id == id }
        pendingFiles.removeAll { $0.id == id }
    }

    /// Photo picks: anything the server's inline-image pipeline does not
    /// accept (HEIC) transcodes to JPEG through the ONE normalizer every
    /// composer shares (EXP-554).
    func ingestPhotos(_ items: [PhotosPickerItem]) async {
        var picks: [ComposerPick<PendingCommentAttachment>] = []
        for item in items {
            guard let data = try? await item.loadTransferable(type: Data.self) else {
                picks.append(.rejected)
                continue
            }
            let type = item.supportedContentTypes.first
            let outcome = AttachmentPicks.normalizedPhoto(
                data: data,
                contentTypeHint: type?.preferredMIMEType,
                filenameExtensionHint: type?.preferredFilenameExtension
            )
            picks.append(outcome.attachment.map(ComposerPick.image) ?? .rejected)
        }
        stageAttachments(picks)
    }

    /// Wave D: Files-app picks, ANY type: an inline image is an image, the
    /// rest file tiles.
    func ingestFiles(_ urls: [URL]) async {
        var picks: [ComposerPick<PendingCommentAttachment>] = []
        for url in urls {
            guard let picked = await AttachmentPicks.readPickedSteerFile(at: url).attachment else {
                picks.append(.rejected)
                continue
            }
            picks.append(
                AttachmentFiles.isInlineImage(contentType: picked.contentType)
                    ? .image(picked) : .file(picked)
            )
        }
        stageAttachments(picks)
    }

    /// EXP-802: a PASTED image (EXP-1635: a camera shot too) joins the strip
    /// like a picked one.
    func ingestPastedImage(_ image: UIImage) {
        guard let data = image.jpegData(compressionQuality: 0.85) else { return }
        let outcome = AttachmentPicks.normalizedPhoto(
            data: data, contentTypeHint: "image/jpeg", filenameExtensionHint: "jpg"
        )
        stageAttachments([outcome.attachment.map(ComposerPick.image) ?? .rejected])
    }
}
#endif
