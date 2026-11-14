// #EXP-1841: THE composer's attachment strip and its `[Image #N]`
// placeholders, shared by the launch composer (a new chat) and every chat's
// composer (session, agent, Assistant, app editor), so the two behave and
// look the same (composer-attachments.json).
import { useRef, useState, type RefObject } from "react"
import { AttachmentFileTile, AttachmentThumb, toast } from "@exp/ui"
import type { MentionTextareaHandle } from "@/components/mention-textarea"
import {
  dropPendingImage,
  stagePendingImages,
  stageToasts,
  type PendingAttachment,
} from "@/lib/pending-images"

/** The strip's thumbnails (an image) and tiles (any other file), in order. */
export function ComposerAttachmentTiles({
  attachments,
  onRemove,
  disabled,
}: {
  attachments: readonly PendingAttachment[]
  onRemove: (url: string) => void
  disabled?: boolean
}) {
  return attachments.map((entry) =>
    entry.kind === `image` ? (
      <AttachmentThumb
        key={entry.url}
        src={entry.url}
        alt={entry.file.name}
        removeLabel="Remove image"
        onRemove={() => onRemove(entry.url)}
        disabled={disabled}
      />
    ) : (
      <AttachmentFileTile
        key={entry.url}
        name={entry.uploadedName ?? entry.file.name}
        removeLabel={`Remove ${entry.file.name}`}
        onRemove={() => onRemove(entry.url)}
        disabled={disabled}
      />
    )
  )
}

/** Stages `files` into `attachments` at the field's caret: the strip grows,
 *  each image's marker lands in the draft, the caret moves behind the last
 *  one, and a refused pick toasts. Returns the strip and the draft after. */
export function stageIntoComposer(args: {
  attachments: PendingAttachment[]
  files: File[]
  text: string
  field: MentionTextareaHandle | null
}): { attachments: PendingAttachment[]; text: string; caret: number } | null {
  if (args.files.length === 0) return null
  const caret = args.field?.caret() ?? args.text.length
  const staged = stagePendingImages(args.attachments, args.files, args.text, caret)
  for (const message of stageToasts(staged)) toast.error(message)
  if (staged.added === 0) return null
  return { attachments: staged.images, text: staged.text, caret: staged.caret }
}

/** A chat composer's pending attachments over its own draft (`text`,
 *  `setText`): the same staging and removal as the launch composer's
 *  (`hooks/use-launch-composer.ts`). */
export function useComposerAttachments(args: {
  textRef: { readonly current: string }
  setText: (text: string) => void
  fieldRef: RefObject<MentionTextareaHandle | null>
}) {
  const [attachments, setAttachmentsState] = useState<PendingAttachment[]>([])
  const ref = useRef(attachments)
  const setAttachments = (next: PendingAttachment[]) => {
    ref.current = next
    setAttachmentsState(next)
  }
  const add = (files: File[]) => {
    const staged = stageIntoComposer({
      attachments: ref.current,
      files,
      text: args.textRef.current,
      field: args.fieldRef.current,
    })
    if (!staged) return
    setAttachments(staged.attachments)
    args.setText(staged.text)
    args.fieldRef.current?.setCaret(staged.caret)
  }
  const remove = (url: string) => {
    const text = args.textRef.current
    const caret = args.fieldRef.current?.caret() ?? text.length
    const dropped = dropPendingImage(ref.current, url, text, caret)
    if (dropped.images === ref.current) return
    URL.revokeObjectURL(url)
    setAttachments(dropped.images)
    // composer-attachments.json `drop`: the caret stays where it was.
    if (dropped.text !== text) {
      args.setText(dropped.text)
      args.fieldRef.current?.setCaret(dropped.caret)
    }
  }
  /** After a send: the entries that went out leave the strip (a pick added
   *  meanwhile stays, composer-send.json). */
  const clearSent = (sent: readonly PendingAttachment[]) => {
    const gone = new Set(sent.map((entry) => entry.url))
    for (const url of gone) URL.revokeObjectURL(url)
    setAttachments(ref.current.filter((entry) => !gone.has(entry.url)))
  }
  return { attachments, ref, add, remove, clearSent }
}
