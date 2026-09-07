import { useEffect, useRef, useState } from "react"
import type { Editor } from "@tiptap/react"
import { useIssueRefs } from "@/components/issue-ref-provider"
import {
  IssuePreviewAnchoredPopover,
  type PreviewAnchor,
} from "@/components/issue-preview-card"

// EXP-760 — the hover preview for `#IDENT` pills INSIDE a markdown editor.
//
// Those pills are ProseMirror inline DECORATIONS (lib/issue-ref-extension.ts):
// the document text stays the plain token, so there is no React element to
// wrap in a HoverCard. This layer does the wrapping at the DOM level instead —
// one delegated listener pair on the editor root, an anchor rect measured from
// whichever `[data-issue-ref]` span the pointer is over, and the shared
// preview card floating off it.
//
// Mounted ONCE per editor (markdown-editor.tsx, desktop only), so it covers
// every editor surface at a stroke: descriptions, comment bodies, the plan
// card and the steering feed's markdown bubbles.

/** Matches the HoverCard defaults so both hosts feel the same. */
const OPEN_DELAY_MS = 400
const CLOSE_DELAY_MS = 100

export function IssueRefHoverLayer({ editor }: { editor: Editor | null }) {
  const issueRefs = useIssueRefs()
  const resolve = issueRefs?.resolve
  const [issueId, setIssueId] = useState<string | null>(null)
  // The identifier the open card belongs to — a transaction re-creates the
  // decoration spans, so re-anchoring reads this rather than a stale node.
  const identifierRef = useRef<string | null>(null)
  const anchorRef = useRef<PreviewAnchor>({
    getBoundingClientRect: () => new DOMRect(),
  })

  useEffect(() => {
    if (!editor || !resolve) return
    const root = editor.view.dom
    let openTimer: ReturnType<typeof setTimeout> | null = null
    let closeTimer: ReturnType<typeof setTimeout> | null = null

    const clearTimers = () => {
      if (openTimer) clearTimeout(openTimer)
      if (closeTimer) clearTimeout(closeTimer)
      openTimer = null
      closeTimer = null
    }

    const close = () => {
      clearTimers()
      identifierRef.current = null
      setIssueId(null)
    }

    /** Point the anchor at the element that carries `identifier`, or close. */
    const anchorTo = (identifier: string): boolean => {
      const node = root.querySelector(
        `[data-issue-ref="${CSS.escape(identifier)}"]`
      )
      if (!node) return false
      anchorRef.current = {
        getBoundingClientRect: () => node.getBoundingClientRect(),
      }
      return true
    }

    const handleOver = (event: PointerEvent) => {
      // Mouse only: on a touch device the pill is a link, and a preview under
      // the finger would just eat the tap.
      if (event.pointerType !== `mouse`) return
      const target =
        event.target instanceof Element
          ? event.target.closest(`[data-issue-ref]`)
          : null
      const identifier = target?.getAttribute(`data-issue-ref`) ?? null
      if (!identifier) return
      if (identifier === identifierRef.current) {
        // Back onto the pill the card already belongs to — cancel the pending
        // close instead of reopening.
        if (closeTimer) clearTimeout(closeTimer)
        closeTimer = null
        return
      }
      const resolved = resolve(identifier)
      if (!resolved) return
      clearTimers()
      openTimer = setTimeout(() => {
        if (!anchorTo(identifier)) return
        identifierRef.current = identifier
        setIssueId(resolved.id)
      }, OPEN_DELAY_MS)
    }

    const handleOut = (event: PointerEvent) => {
      if (event.pointerType !== `mouse`) return
      const from =
        event.target instanceof Element
          ? event.target.closest(`[data-issue-ref]`)
          : null
      if (!from) return
      // Still inside the same pill (crossing a child node) — not a leave.
      const to =
        event.relatedTarget instanceof Element
          ? event.relatedTarget.closest(`[data-issue-ref]`)
          : null
      if (to === from) return
      if (openTimer) clearTimeout(openTimer)
      openTimer = null
      if (closeTimer) clearTimeout(closeTimer)
      closeTimer = setTimeout(close, CLOSE_DELAY_MS)
    }

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === `Escape`) close()
    }

    // The anchor is a live rect, but the DECORATION SPAN is not: a transaction
    // rebuilds it, and the old node then reports a stale (or zero) rect
    // forever. Re-resolve after every update, and close when the token the
    // card belongs to is gone from the document.
    const handleUpdate = () => {
      const identifier = identifierRef.current
      if (!identifier) return
      if (!anchorTo(identifier)) close()
    }

    root.addEventListener(`pointerover`, handleOver)
    root.addEventListener(`pointerout`, handleOut)
    window.addEventListener(`scroll`, close, true)
    window.addEventListener(`resize`, close)
    window.addEventListener(`keydown`, handleKeyDown)
    editor.on(`update`, handleUpdate)
    editor.on(`destroy`, close)

    return () => {
      clearTimers()
      root.removeEventListener(`pointerover`, handleOver)
      root.removeEventListener(`pointerout`, handleOut)
      window.removeEventListener(`scroll`, close, true)
      window.removeEventListener(`resize`, close)
      window.removeEventListener(`keydown`, handleKeyDown)
      editor.off(`update`, handleUpdate)
      editor.off(`destroy`, close)
    }
  }, [editor, resolve])

  if (!issueId) return null
  return (
    <IssuePreviewAnchoredPopover
      issueId={issueId}
      anchorRef={anchorRef}
      onClose={() => {
        identifierRef.current = null
        setIssueId(null)
      }}
    />
  )
}
