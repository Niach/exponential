import ExpCore
import ExpUI
import PhotosUI
import SwiftUI
import UIKit

// #EXP-1841 (composer-attachments.json): THE attachment strip of EVERY
// composer — a new chat's (`AgentComposerModel`, "To <agent>") and every
// chat's (`SessionComposerDraft`: a session, an agent chat, the Assistant, an
// app editor). One path from a pick (photo picker, camera, Files, paste;
// several at once) to the strip: classify here, stage through
// `ComposerAttachments` (the `[Image #N]` marker at the caret, the caps), toast
// what stayed out; the thumbnail's X drops the entry and renumbers.

/// One picked file, classified (an inline image, any other file, or refused
/// for its size or type).
enum ComposerPick {
    case image(PendingCommentAttachment)
    case file(PendingCommentAttachment)
    case rejected

    var kind: ComposerAttachments.Pick {
        switch self {
        case .image: return .image
        case .file: return .file
        case .rejected: return .rejected
        }
    }
}

/// A composer that holds a draft and an attachment strip.
@MainActor
protocol ComposerAttachmentHost: AnyObject {
    var draftEditor: IssueEditorModel { get }
    var pendingImages: [PendingSteerImage] { get set }
    var pendingFiles: [PendingSteerFile] { get set }
    /// The strip took a pick (the launch composer's "touched").
    func attachmentsStaged()
}

extension ComposerAttachmentHost {
    func attachmentsStaged() {}

    /// The strip as kinds. Numbering and caps only count per kind, so the
    /// images-then-files order the strip draws is the fixture's strip.
    var attachmentStrip: [ComposerAttachments.Kind] {
        Array(repeating: .image, count: pendingImages.count)
            + Array(repeating: .file, count: pendingFiles.count)
    }

    var imagesFull: Bool { pendingImages.count >= ComposerAttachments.maxImages }
    var filesFull: Bool { pendingFiles.count >= ComposerAttachments.maxFiles }
    /// Both lists full: nothing more can be attached (the "+" dims).
    var attachFull: Bool { imagesFull && filesFull }

    /// Stage one pick (in pick order): each taken image drops its marker at
    /// the caret, the caret lands behind the last one, and a pick over a cap
    /// or refused toasts.
    func stageAttachments(_ picks: [ComposerPick]) {
        guard !picks.isEmpty else { return }
        let text = draftEditor.plainText
        let caret = draftEditor.plainTextCaret ?? (text as NSString).length
        let staged = ComposerAttachments.stage(
            strip: attachmentStrip, add: picks.map(\.kind), text: text, caret: caret
        )
        for (pick, taken) in zip(picks, staged.taken) where taken {
            switch pick {
            case let .image(picked):
                pendingImages.append(PendingSteerImage(
                    data: picked.data,
                    filename: picked.filename,
                    contentType: picked.contentType,
                    uploadedId: nil
                ))
            case let .file(picked):
                pendingFiles.append(PendingSteerFile(
                    data: picked.data, filename: picked.filename, contentType: picked.contentType
                ))
            case .rejected:
                break
            }
        }
        if staged.text != text { draftEditor.setPlainText(staged.text, caret: staged.caret) }
        if staged.taken.contains(true) { attachmentsStaged() }
        for message in staged.toasts {
            Toaster.shared.show(ToastItem(kind: .error, title: message))
        }
    }

    /// The thumbnail's X: an image's markers leave the draft and the higher
    /// ones slide down; a file's leaves the text alone. The caret stays where
    /// it was, moved left by what left before it.
    func removeAttachment(id: UUID) {
        if let position = pendingImages.firstIndex(where: { $0.id == id }) {
            let text = draftEditor.plainText
            let caret = draftEditor.plainTextCaret ?? (text as NSString).length
            let dropped = ComposerAttachments.drop(
                strip: attachmentStrip, at: position, text: text, caret: caret
            )
            pendingImages.remove(at: position)
            if dropped.text != text { draftEditor.setPlainText(dropped.text, caret: dropped.caret) }
        } else {
            pendingFiles.removeAll { $0.id == id }
        }
    }

    /// Photo picks: anything the server's inline-image pipeline does not
    /// accept (HEIC) transcodes to JPEG through the ONE normalizer every
    /// composer shares (EXP-554).
    func ingestPhotos(_ items: [PhotosPickerItem]) async {
        var picks: [ComposerPick] = []
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
        var picks: [ComposerPick] = []
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
