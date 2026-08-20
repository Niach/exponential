import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import type { Editor } from "@tiptap/react"
import { BubbleMenu } from "@tiptap/react/menus"
import { isTextSelection } from "@tiptap/core"
import { PluginKey } from "@tiptap/pm/state"
import {
  FormattingRail,
  type FormattingRailMode,
} from "@/components/issue-editor/formatting-rail"
import type { MarkdownEditorImageUploadConfig } from "@/components/issue-editor/markdown-editor"

// EXP-568 — the desktop editor chrome: the formatting rail floating over a
// text selection instead of a toolbar band nailed above every editor. One
// module-level plugin key so the hide/reposition metas below address exactly
// this menu (the BubbleMenu component would otherwise mint an anonymous one
// per mount and there would be nothing to dispatch against).
const formattingRailPluginKey = new PluginKey(`formattingRail`)

// Rendering into <body> is what keeps the rail out of every clipping scroll
// region (the dialog body, the detail page's scrollport) — the same escape
// the @/# autocomplete popup makes.
const appendToBody = () => document.body

export function EditorSelectionRail({
  editor,
  imageUpload,
}: {
  editor: Editor
  imageUpload?: MarkdownEditorImageUploadConfig
}) {
  const [mode, setMode] = useState<FormattingRailMode>(`main`)
  const modeRef = useRef(mode)
  modeRef.current = mode
  // True while a rail-owned overlay (emoji popover / a dropdown) holds focus.
  // Those portal OUTSIDE the rail element, so without this latch the editor
  // blur they cause would tear the rail down under the open menu.
  const overlayLockRef = useRef(false)

  const shouldShow = useCallback(
    ({
      editor: instance,
      state,
      view,
      from,
      to,
    }: {
      editor: Editor
      state: Editor[`state`]
      view: Editor[`view`]
      from: number
      to: number
    }) => {
      if (overlayLockRef.current || modeRef.current !== `main`) return true
      if (!instance.isEditable || !view.hasFocus()) return false
      const { selection } = state
      if (selection.empty || !isTextSelection(selection)) return false
      return state.doc.textBetween(from, to).length > 0
    },
    []
  )

  // Stable identities: the React BubbleMenu dispatches an `updateOptions`
  // transaction whenever any of these change, so a fresh literal per render
  // would put a transaction on every render.
  const floatingOptions = useMemo(
    () => ({
      strategy: `fixed` as const,
      placement: `top-start` as const,
      offset: 6,
      flip: {},
      shift: {},
    }),
    []
  )

  // `appendTo: body` makes the plugin's own blur-hide a no-op — it keeps the
  // menu whenever the focus target lives inside the element's parent, which
  // is now the entire document. Hide it ourselves, unless the focus went to
  // the rail (the link input) or an overlay the rail opened.
  useEffect(() => {
    if (typeof editor?.on !== `function`) return
    const onBlur = ({ event }: { event: FocusEvent }) => {
      if (overlayLockRef.current || modeRef.current !== `main`) return
      const related = event?.relatedTarget
      if (
        related instanceof Element &&
        related.closest(`[data-editor-rail]`) !== null
      ) {
        return
      }
      if (editor.isDestroyed) return
      setMode(`main`)
      editor.view.dispatch(
        editor.state.tr.setMeta(formattingRailPluginKey, `hide`)
      )
    }
    editor.on(`blur`, onBlur)
    return () => {
      editor.off(`blur`, onBlur)
    }
  }, [editor])

  // A mode swap resizes the rail; nothing in the plugin notices, so ask it to
  // recompute its position.
  useEffect(() => {
    if (typeof editor?.on !== `function` || editor.isDestroyed) return
    editor.view.dispatch(
      editor.state.tr.setMeta(formattingRailPluginKey, `updatePosition`)
    )
  }, [editor, mode])

  return (
    <BubbleMenu
      editor={editor}
      pluginKey={formattingRailPluginKey}
      updateDelay={150}
      appendTo={appendToBody}
      shouldShow={shouldShow}
      options={floatingOptions}
      data-editor-rail=""
      className="formatting-rail glass-panel pointer-events-auto rounded-xl"
      style={{ zIndex: 60 }}
    >
      <FormattingRail
        editor={editor}
        imageUpload={imageUpload}
        platform="desktop"
        mode={mode}
        onModeChange={setMode}
        onOverlayOpenChange={(open) => {
          overlayLockRef.current = open
        }}
      />
    </BubbleMenu>
  )
}
