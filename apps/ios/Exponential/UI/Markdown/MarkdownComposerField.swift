import ExpUI
import SwiftUI
import UIKit

/// EXP-802 — the ONE chat-shaped markdown field: a `BlockTextEditor` over a
/// one-text-block `IssueEditorModel`, so a composer that is not the issue
/// comment box still gets the `@` member / `#` issue / `:` emoji typeahead.
///
/// Why not `GlassTextField`: it wraps SwiftUI's `TextField`, which binds a
/// `String` and exposes NO selection. Nothing can splice a picked candidate at
/// the caret through it — the same reason a steer photo pick's `[Image #k]`
/// marker had to land at the END of the draft. A typeahead needs a caret, and
/// on iOS only a `UITextView` has one.
///
/// Why not `MarkdownEditor`: that renders a DOCUMENT — Return splits blocks,
/// images and tables become blocks of their own, the formatting strip rides
/// the keyboard. This is one message that sends. `singleLine` keeps the return
/// key out of the storage and hands it to `onReturn` instead, so the model can
/// never grow a second block and `IssueEditorModel.plainText` is exactly what
/// was typed.
///
/// The host mounts the candidate menu itself (`EditorAutocompleteMenu`), above
/// the keyboard — see the note in `MarkdownEditor` for why an in-editor anchor
/// cannot be made to work.
struct MarkdownComposerField: View {
    let model: IssueEditorModel
    var placeholder: String = ""
    /// The return key (soft or hardware). Nil swallows it, which is what keeps
    /// a newline out of the one block either way.
    var onReturn: (() -> Void)?
    /// A pasted image. The host decides what one MEANS — the steer composer
    /// queues it as a pending attachment, since an image block would break the
    /// single-block contract above. Nil ignores the paste.
    var onPasteImage: ((UIImage) -> Void)?
    var onIssueRefTap: ((String) -> Void)?
    /// The comment composer's band: one line, growing to about four before the
    /// field scrolls inside itself.
    var minHeight: CGFloat = 44
    var maxHeight: CGFloat = 140

    /// The block this field edits. `IssueEditorModel` starts with exactly one
    /// empty text block and `singleLine` stops a second one ever appearing, so
    /// this only ever misses if a host loaded a document into a composer.
    private var textBlock: (id: UUID, content: NSAttributedString)? {
        for block in model.blocks {
            if case let .text(id, content) = block { return (id, content) }
        }
        return nil
    }

    var body: some View {
        if let block = textBlock {
            BlockTextEditor(
                model: model,
                blockId: block.id,
                content: block.content,
                revision: model.revision(for: block.id),
                isFocused: model.focusedBlockId == block.id,
                placeholder: placeholder,
                // No formatting accessory: the same call the comment composer
                // makes (EXP-246). A chat field offers bold and quote nothing.
                toolbar: nil,
                singleLine: true,
                returnKeyType: .send,
                flattensPastedNewlines: false,
                onReturn: onReturn,
                onPasteImage: { image in onPasteImage?(image) },
                onIssueRefTap: onIssueRefTap
            )
            .boundedEditorHeight(minHeight: minHeight, maxHeight: maxHeight)
        }
    }
}
