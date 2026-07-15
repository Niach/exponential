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
  | { kind: `success`; identifier: string | null; url: string | null }

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
  // True while the annotator is editing a capture the reporter never
  // confirmed (straight from "Take screenshot"): cancelling then discards the
  // image. Re-edits of an already-attached shot (the Annotate chip) keep it.
  const freshCaptureRef = useRef(false)
  // A flatten (image decode + canvas re-encode) can take a second on slow
  // devices. Submitting during that window must never send the pristine base
  // screenshot — it may contain content the reporter deliberately cropped
  // away — so the in-flight promise is kept for submit to await and the
  // boolean disables the Send button meanwhile.
  const [flattening, setFlattening] = useState(false)
  const pendingFlattenRef = useRef<Promise<Blob | null> | null>(null)
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
    // Any flatten still encoding belongs to the replaced screenshot; submit
    // must not await (or use) its result.
    pendingFlattenRef.current = null
    setFlattening(false)
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
    // Screenshots are on demand: the panel opens straight onto the form and
    // capturing only happens when the reporter asks for it.
    setCaptureFailed(false)
    setPhase({ kind: `open` })
  }, [])

  const close = useCallback(() => {
    // The form fields die with the unmounting Panel; keeping the screenshot
    // (a snapshot of a page state that may be long gone by the next open)
    // would be inconsistent, so it goes too.
    replaceBase(null)
    setCaptureFailed(false)
    setPhase({ kind: `closed` })
  }, [replaceBase])

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

  // The loader resolves the remote config in parallel with this bundle; when
  // the panel wins that race it renders from a null config. Re-render once
  // the config lands so gates like emailRequired (and remote accent/label)
  // reflect the board's real settings. The loader's own `.then` (registered
  // at init, before this bundle could load) has already written state.config
  // by the time this continuation runs.
  useEffect(() => {
    let cancelled = false
    void state.configPromise.then(() => {
      if (!cancelled) bumpVersion((version) => version + 1)
    })
    return () => {
      cancelled = true
    }
  }, [state])

  const retake = useCallback(() => {
    // Close the panel, recapture without it, reopen.
    setPhase({ kind: `capturing` })
    requestAnimationFrame(() => {
      void capture().then(() => setPhase({ kind: `open` }))
    })
  }, [capture])

  const takeScreenshot = useCallback(() => {
    // Hide the panel, capture the page as the reporter sees it, then land in
    // the annotation editor — capturing is an explicit intent to mark up.
    setPhase({ kind: `capturing` })
    requestAnimationFrame(() => {
      void capture().then((captured) => {
        freshCaptureRef.current = captured
        setPhase(captured ? { kind: `annotating` } : { kind: `open` })
      })
    })
  }, [capture])

  const openAnnotator = useCallback(() => {
    if (!baseRef.current) return
    if (phaseRef.current.kind !== `open`) return
    freshCaptureRef.current = false
    setPhase({ kind: `annotating` })
  }, [])

  const cancelAnnotate = useCallback(() => {
    // Cancelling out of a never-confirmed capture discards it — it must not
    // stay silently attached to the submission.
    if (freshCaptureRef.current) {
      freshCaptureRef.current = false
      replaceBase(null)
    }
    setPhase({ kind: `open` })
  }, [replaceBase])

  const saveAnnotations = useCallback(
    async (next: AnnotationShape[], nextCrop: NormalizedRect | null) => {
      // Saving confirms the capture: later cancels keep the shot attached.
      freshCaptureRef.current = false
      setPhase({ kind: `open` })
      setShapes(next)
      setCrop(nextCrop)
      setAnnotated((previous) => {
        if (previous) URL.revokeObjectURL(previous.objectUrl)
        return null
      })
      const currentBase = baseRef.current
      if (!currentBase || (next.length === 0 && !nextCrop)) return
      const pending = flattenAnnotations(currentBase.blob, next, nextCrop)
      pendingFlattenRef.current = pending
      setFlattening(true)
      const blob = await pending
      // A retake/remove or a newer save may have superseded this flatten.
      if (pendingFlattenRef.current === pending) {
        pendingFlattenRef.current = null
        setFlattening(false)
      }
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
      // A flatten can still be encoding here (the disabled Send button can
      // race a stale render): await it and send ITS result — even a null
      // encode failure. During the pending window this closure's `screenshot`
      // is the pristine base, so falling back to it would leak content the
      // reporter cropped away; sending no screenshot fails closed.
      const pendingFlatten = pendingFlattenRef.current
      const screenshotBlob = pendingFlatten
        ? await pendingFlatten
        : (screenshot?.blob ?? null)
      const result = await submitFeedback({
        state,
        title: form.title,
        description: form.description,
        email: form.email || state.identity.email || null,
        screenshot: screenshotBlob,
        meta: collectEnvMeta(),
      })
      if (result.ok) {
        replaceBase(null)
        setCaptureFailed(false)
        setPhase({
          kind: `success`,
          identifier: result.identifier,
          url: result.url,
        })
        // Leave the success card up longer when it carries a link to the
        // public issue, so the reporter has a chance to click through.
        window.setTimeout(() => {
          setPhase((current) =>
            current.kind === `success` ? { kind: `closed` } : current
          )
        }, result.url ? 6_000 : 2_500)
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
          successUrl={phase.kind === `success` ? phase.url : null}
          position={position}
          screenshot={screenshot}
          flattening={flattening}
          captureFailed={captureFailed}
          identityEmail={state.identity.email ?? null}
          emailRequired={state.config?.form?.emailRequired === true}
          onClose={close}
          onCapture={takeScreenshot}
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
