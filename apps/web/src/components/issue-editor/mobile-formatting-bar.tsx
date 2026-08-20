import { useEffect, useRef, useState } from "react"
import { createPortal } from "react-dom"
import type { Editor } from "@tiptap/react"
import {
  FormattingRail,
  type FormattingRailMode,
} from "@/components/issue-editor/formatting-rail"
import type { MarkdownEditorImageUploadConfig } from "@/components/issue-editor/markdown-editor"
import { useKeyboardInset } from "@/hooks/use-keyboard-inset"
import { cn } from "@/lib/utils"

// EXP-568 — the phone editor chrome: the formatting rail riding the top edge
// of the on-screen keyboard, native-composer style. It exists only while the
// description editor is focused (or while the rail's own overlay is up), so a
// phone reading an issue never loses a row of screen to formatting glyphs.
//
// The bar deliberately has NO transition on `bottom`: the keyboard's own
// open/close animation already moves the inset, and a second easing on top of
// it reads as lag.

export function EditorMobileFormattingBar({
  editor,
  imageUpload,
}: {
  editor: Editor
  imageUpload?: MarkdownEditorImageUploadConfig
}) {
  const { inset } = useKeyboardInset()
  const [mode, setMode] = useState<FormattingRailMode>(`main`)
  const [focused, setFocused] = useState(false)
  const [overlayOpen, setOverlayOpen] = useState(false)
  const blurFrameRef = useRef(0)

  useEffect(() => {
    if (typeof editor?.on !== `function`) return
    setFocused(editor.isFocused)
    const onFocus = () => {
      cancelAnimationFrame(blurFrameRef.current)
      setFocused(true)
    }
    // Opening a file picker (or a portalled menu) blurs the editor for a
    // frame before focus comes back — hiding synchronously would flicker the
    // bar out from under the tap that opened it.
    const onBlur = () => {
      cancelAnimationFrame(blurFrameRef.current)
      blurFrameRef.current = requestAnimationFrame(() => {
        if (editor.isDestroyed || editor.isFocused) return
        setFocused(false)
      })
    }
    editor.on(`focus`, onFocus)
    editor.on(`blur`, onBlur)
    return () => {
      cancelAnimationFrame(blurFrameRef.current)
      editor.off(`focus`, onFocus)
      editor.off(`blur`, onBlur)
    }
  }, [editor])

  // A non-main mode is a live interaction of its own (the link editor owns
  // the focus), so it keeps the bar up on its own account.
  const visible = focused || overlayOpen || mode !== `main`
  useEffect(() => {
    if (!visible && mode !== `main`) setMode(`main`)
  }, [visible, mode])

  if (!visible || typeof document === `undefined`) return null

  return createPortal(
    <div
      data-editor-rail=""
      className={cn(
        `formatting-rail fixed inset-x-0 z-40 flex items-center gap-px overflow-x-auto border-t border-glass-stroke-card px-2 py-1.5 md:hidden`,
        `glass-chrome-bottom`,
        // Only clear the home indicator when the keyboard is NOT holding the
        // bar up off the screen edge.
        inset === 0 && `pb-[max(0.375rem,env(safe-area-inset-bottom))]`
      )}
      style={{ bottom: inset }}
    >
      <FormattingRail
        editor={editor}
        imageUpload={imageUpload}
        platform="mobile"
        mode={mode}
        onModeChange={setMode}
        onOverlayOpenChange={setOverlayOpen}
        onDismissKeyboard={() => editor.commands.blur()}
      />
    </div>,
    document.body
  )
}
