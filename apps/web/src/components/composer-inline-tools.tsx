import { useImperativeHandle, useRef, type Ref } from "react"

// EXP-1334 / S-3 (composer-menu.json): every composer's "+" is its ONLY
// tool: no inline image / file / `#` / emoji buttons (emoji stays only in
// the issue comment composer). What stays here is the hidden file input
// the "+" menu's "Photo or file" row opens through the handle (no type
// filter: any file).

export interface ComposerInlineToolsHandle {
  /** Opens the any-file chooser (the "+" menu's "Photo or file"). */
  attach: () => void
}

export function ComposerInlineTools({
  ref,
  onFiles,
}: {
  ref?: Ref<ComposerInlineToolsHandle>
  onFiles: (files: File[]) => void
}) {
  const fileInputRef = useRef<HTMLInputElement>(null)
  useImperativeHandle(ref, () => ({ attach: () => fileInputRef.current?.click() }))

  return (
    <input
      ref={fileInputRef}
      type="file"
      multiple
      hidden
      data-testid="composer-file-input"
      onChange={(event) => {
        const input = event.target
        if (input.files) onFiles(Array.from(input.files))
        input.value = ``
      }}
    />
  )
}
