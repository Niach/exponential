// #EDE-65 R: a paste into the Composer's field in the DOM, shared by the
// React and Solid Composer (fixtures/interactions-round1.json
// `composerPaste`): image files fire `attach` {files: [{name, size, type}]}
// (only the images, clipboard order; FileUpload's file shape) and go to the
// host's `onUpload`; the field inserts nothing. Files without an image do
// nothing; a paste without files is the field's own.

/** The file shape `attach` reports (FileUpload's `upload` shape). */
export interface ComposerFileMeta {
  name: string
  size: number
  type: string
}

/** What a paste does: `take` = the field inserts nothing (prevent the
 *  default); `images` = the files to attach (empty = no `attach`). */
export interface ComposerPaste {
  take: boolean
  images: File[]
  files: ComposerFileMeta[]
}

/** Decide a paste from its clipboard data (the event's `clipboardData`). */
export function composerPaste(data: Pick<DataTransfer, `files`> | null | undefined): ComposerPaste {
  const all = Array.from(data?.files ?? [])
  const images = all.filter((f) => f.type.startsWith(`image/`))
  return { take: all.length > 0, images, files: images.map((f) => ({ name: f.name, size: f.size, type: f.type })) }
}
