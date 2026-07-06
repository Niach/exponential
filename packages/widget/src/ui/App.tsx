import { useCallback, useEffect, useRef, useState } from "preact/hooks"
import type { WidgetRuntimeState } from "../types"
import type { AnnotationShape, NormalizedRect } from "../annotate/shapes"
import { flattenAnnotations } from "../annotate/flatten"
import { captureScreenshot } from "../capture/engine"
import { snapdomEngine } from "../capture/snapdom-engine"
import { collectEnvMeta } from "../env-meta"
import { submitFeedback } from "../api-client"
import { megaphoneIconSvg, pickForeground, theme } from "../theme"
import { Annotator } from "./Annotator"
import { Panel } from "./Panel"

type UiPhase =
  | { kind: `closed` }
  | { kind: `capturing` }
  | { kind: `open` }
  | { kind: `annotating` }
  | { kind: `submitting` }
  | { kind: `success`; identifier: string | null }

export interface Screenshot {
  blob: Blob
  objectUrl: string
}

export function App({ state }: { state: WidgetRuntimeState }) {
  const [phase, setPhase] = useState<UiPhase>({ kind: `closed` })
  // `base` is the pristine capture annotations are drawn over; `annotated`
  // is the flattened result (what the preview shows and submit sends).
  // Shapes are kept so reopening the editor stays non-destructive.
  const [base, setBase] = useState<Screenshot | null>(null)
  const [annotated, setAnnotated] = useState<Screenshot | null>(null)
  const [shapes, setShapes] = useState<AnnotationShape[]>([])
  // Crop rect in the ORIGINAL screenshot's pixel space (null = uncropped).
  // Kept alongside shapes so reopening the editor stays non-destructive and
  // recropping is possible right up to submit.
  const [crop, setCrop] = useState<NormalizedRect | null>(null)
  const [captureFailed, setCaptureFailed] = useState(false)
  // Re-render when identify()/setCustomData() land after mount.
  const [, bumpVersion] = useState(0)
  const phaseRef = useRef(phase)
  phaseRef.current = phase
  const baseRef = useRef(base)
  baseRef.current = base

  const screenshot = annotated ?? base

  const accent =
    state.options.color ?? state.config?.form?.accentColor ?? theme.defaultAccent
  const position =
    state.options.position ?? state.config?.form?.position ?? `bottom-left`
  const label = state.options.label ?? state.config?.form?.buttonLabel ?? `Feedback`

  const replaceBase = useCallback((next: Screenshot | null) => {
    setBase((previous) => {
      if (previous) URL.revokeObjectURL(previous.objectUrl)
      return next
    })
    setAnnotated((previous) => {
      if (previous) URL.revokeObjectURL(previous.objectUrl)
      return null
    })
    setShapes([])
    setCrop(null)
  }, [])

  const capture = useCallback(async (): Promise<boolean> => {
    const blob = await captureScreenshot(snapdomEngine)
    if (blob) {
      replaceBase({ blob, objectUrl: URL.createObjectURL(blob) })
      setCaptureFailed(false)
      return true
    }
    replaceBase(null)
    setCaptureFailed(true)
    return false
  }, [replaceBase])

  const open = useCallback(() => {
    if (phaseRef.current.kind !== `closed`) return
    // Capture BEFORE the panel renders so the screenshot shows the page as
    // the reporter sees it, panel-free.
    setPhase({ kind: `capturing` })
    void capture().then((captured) => {
      if (phaseRef.current.kind !== `capturing`) return
      // Jump straight into the annotation editor when a screenshot exists —
      // marking up the shot is the most common first action. Happens exactly
      // once per open: closing the editor lands on the form and nothing
      // re-triggers it.
      setPhase(captured ? { kind: `annotating` } : { kind: `open` })
    })
  }, [capture])

  const close = useCallback(() => {
    setPhase({ kind: `closed` })
  }, [])

  useEffect(() => {
    state.bundle = {
      open,
      close,
      stateChanged: () => bumpVersion((version) => version + 1),
    }
    if (state.openRequested) {
      state.openRequested = false
      open()
    }
    return () => {
      state.bundle = null
    }
  }, [state, open, close])

  const retake = useCallback(() => {
    // Close the panel, recapture without it, reopen.
    setPhase({ kind: `capturing` })
    requestAnimationFrame(() => {
      void capture().then(() => setPhase({ kind: `open` }))
    })
  }, [capture])

  const openAnnotator = useCallback(() => {
    if (!baseRef.current) return
    if (phaseRef.current.kind !== `open`) return
    setPhase({ kind: `annotating` })
  }, [])

  const cancelAnnotate = useCallback(() => {
    setPhase({ kind: `open` })
  }, [])

  const saveAnnotations = useCallback(
    async (next: AnnotationShape[], nextCrop: NormalizedRect | null) => {
      setPhase({ kind: `open` })
      setShapes(next)
      setCrop(nextCrop)
      setAnnotated((previous) => {
        if (previous) URL.revokeObjectURL(previous.objectUrl)
        return null
      })
      const currentBase = baseRef.current
      if (!currentBase || (next.length === 0 && !nextCrop)) return
      const blob = await flattenAnnotations(currentBase.blob, next, nextCrop)
      // The shot may have been retaken/removed while encoding.
      if (baseRef.current !== currentBase) return
      if (blob) {
        setAnnotated({ blob, objectUrl: URL.createObjectURL(blob) })
      } else {
        // Encode failed: fall back to the clean screenshot instead of lying
        // about what will be submitted.
        setShapes([])
        setCrop(null)
      }
    },
    []
  )

  const submit = useCallback(
    async (form: { title: string; description: string; email: string }) => {
      setPhase({ kind: `submitting` })
      const result = await submitFeedback({
        state,
        title: form.title,
        description: form.description,
        email: form.email || state.identity.email || null,
        screenshot: screenshot?.blob ?? null,
        meta: collectEnvMeta(),
      })
      if (result.ok) {
        replaceBase(null)
        setCaptureFailed(false)
        setPhase({ kind: `success`, identifier: result.identifier })
        window.setTimeout(() => {
          setPhase((current) =>
            current.kind === `success` ? { kind: `closed` } : current
          )
        }, 2_500)
        return null
      }
      setPhase({ kind: `open` })
      return result.message
    },
    [state, screenshot, replaceBase]
  )

  const showButton =
    state.options.showButton !== false && phase.kind !== `annotating`
  const panelVisible =
    phase.kind === `open` ||
    phase.kind === `submitting` ||
    phase.kind === `success`
  // Keep the Panel mounted (display:none) while annotating so the typed
  // title/description survive the round-trip into the editor.
  const panelMounted = panelVisible || phase.kind === `annotating`

  return (
    <div
      className="exp-root"
      style={{
        "--exp-font": theme.font,
        "--exp-background": theme.background,
        "--exp-card": theme.card,
        "--exp-secondary": theme.secondary,
        "--exp-foreground": theme.foreground,
        "--exp-muted-foreground": theme.mutedForeground,
        "--exp-border": theme.border,
        "--exp-input": theme.input,
        "--exp-destructive": theme.destructive,
        "--exp-success": theme.success,
        "--exp-radius": theme.radius,
        "--exp-accent": accent,
        "--exp-accent-foreground": pickForeground(accent),
      }}
    >
      {showButton && (
        <div
          style={{
            position: `fixed`,
            bottom: `20px`,
            [position === `bottom-left` ? `left` : `right`]: `20px`,
          }}
        >
          <button
            className="exp-fab"
            aria-label="Send feedback"
            aria-haspopup="dialog"
            aria-expanded={panelVisible}
            onClick={() => (panelVisible ? close() : open())}
          >
            <span
              style={{ display: `flex` }}
              // eslint-disable-next-line react/no-danger
              dangerouslySetInnerHTML={{ __html: megaphoneIconSvg }}
            />
            {label ? <span className="exp-fab-label">{label}</span> : null}
          </button>
        </div>
      )}

      {panelMounted && (
        <Panel
          phase={phase.kind === `annotating` ? `open` : phase.kind}
          hidden={phase.kind === `annotating`}
          successIdentifier={
            phase.kind === `success` ? phase.identifier : null
          }
          position={position}
          screenshot={screenshot}
          captureFailed={captureFailed}
          identityEmail={state.identity.email ?? null}
          emailRequired={state.config?.form?.emailRequired === true}
          onClose={close}
          onRetake={retake}
          onAnnotate={openAnnotator}
          onRemoveScreenshot={() => replaceBase(null)}
          onSubmit={submit}
        />
      )}

      {phase.kind === `annotating` && base && (
        <Annotator
          imageUrl={base.objectUrl}
          initialShapes={shapes}
          initialCrop={crop}
          onCancel={cancelAnnotate}
          onSave={(next, nextCrop) => void saveAnnotations(next, nextCrop)}
        />
      )}
    </div>
  )
}
