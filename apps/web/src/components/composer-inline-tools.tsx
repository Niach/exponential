import { useImperativeHandle, useRef, type Ref } from "react"
import type { ComposerFileChoiceId } from "@/components/launch-dialog/composer-menu"

// EXP-1334 / S-3 (composer-menu.json): every composer's "+" is its ONLY
// tool: no inline image / file / `#` / emoji buttons (emoji stays only in
// the issue comment composer). What stays here are the hidden file inputs
// the "+" menu's "Photo or file" row opens through the handle. EXP-1635: one
// input per `addFile.phoneChoices` entry: `file` (no type filter, what a
// pointer opens directly), `photos` (images, many) and `camera` (one shot).

export interface ComposerInlineToolsHandle {
  /** Opens a chooser (the "+" menu's "Photo or file"); default = any file. */
  attach: (choice?: ComposerFileChoiceId) => void
}

const FILE_INPUTS = {
  file: { multiple: true, testId: `composer-file-input` },
  photos: { accept: `image/*`, multiple: true, testId: `composer-photo-input` },
  camera: { accept: `image/*`, capture: `environment`, testId: `composer-camera-input` },
} satisfies Record<
  ComposerFileChoiceId,
  { accept?: string; capture?: `environment`; multiple?: boolean; testId: string }
>

const FILE_INPUT_CHOICES = Object.keys(FILE_INPUTS) as ComposerFileChoiceId[]

export function ComposerInlineTools({
  ref,
  onFiles,
}: {
  ref?: Ref<ComposerInlineToolsHandle>
  onFiles: (files: File[]) => void
}) {
  const inputs = useRef<Partial<Record<ComposerFileChoiceId, HTMLInputElement | null>>>({})
  useImperativeHandle(ref, () => ({
    attach: (choice = `file`) => inputs.current[choice]?.click(),
  }))

  return (
    <>
      {FILE_INPUT_CHOICES.map((choice) => {
        const { testId, ...attributes } = FILE_INPUTS[choice]
        return (
          <input
            key={choice}
            ref={(node) => {
              inputs.current[choice] = node
            }}
            type="file"
            {...attributes}
            hidden
            data-testid={testId}
            onChange={(event) => {
              const input = event.target
              if (input.files) onFiles(Array.from(input.files))
              input.value = ``
            }}
          />
        )
      })}
    </>
  )
}
