import { useCallback, useEffect, useRef, useState } from "preact/hooks"
import type {
  WidgetMode,
  WidgetRemoteConfig,
  WidgetRuntimeState,
} from "../types"
import type { AnnotationShape, NormalizedRect } from "../annotate/shapes"
import { flattenAnnotations } from "../annotate/flatten"
import { captureScreenshot } from "../capture/engine"
import { snapdomEngine } from "../capture/snapdom-engine"
import { collectEnvMeta } from "../env-meta"
import { submitFeedback, submitSupportRequest } from "../api-client"
import { megaphoneIconSvg, pickForeground, theme } from "../theme"
import { Annotator } from "./Annotator"
import { Panel } from "./Panel"
import type { PanelView } from "./Panel"

type UiPhase =
  | { kind: `closed` }
  | { kind: `capturing` }
  | { kind: `open` }
  | { kind: `annotating` }
  | { kind: `submitting` }
  | {
      kind: `success`
      flavor: WidgetMode
      identifier: string | null
      url: string | null
    }

// The panel's entry points, from the remote config. Absent / unknown values
// (older servers, cache skew) degrade to feedback-only — today's behavior.
function effectiveModes(config: WidgetRemoteConfig | null): WidgetMode[] {
  const modes =
    config?.modes?.filter(
      (mode) => mode === `feedback` || mode === `support`
    ) ?? []
  return modes.length > 0 ? modes : [`feedback`]
}

export interface Screenshot {
  blob: Blob
  objectUrl: string
}

// A failure that implicates the email address: the structured code from
// current servers, or — for old self-hosted servers that predate codes — a
// bare 400 whose message matches their frozen email-failure copy. Other
// code-less 400s (oversized meta/customData, bad screenshot) must NOT
// discard a valid identity email: revealing the field would blame the
// address for a failure it can't fix.
function isEmailFailure(result: {
  status: number | null
  code: string | null
  message: string
}): boolean {
  if (result.code === `invalid_email` || result.code === `email_required`) {
    return true
  }
  return (
    result.code === null &&
    result.status === 400 &&
    (result.message === `Invalid submission fields` ||
      result.message === `Email is required`)
  )
}

export function App({ state }: { state: WidgetRuntimeState }) {
  const [phase, setPhase] = useState<UiPhase>({ kind: `closed` })
  // Which pane the panel shows: the card home (both modes), or one form
  // directly (single mode) — set at open time from the resolved config.
  const [view, setView] = useState<PanelView>(`feedback`)
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
  // The identity email the server refused: while set, the derived
  // identityEmail below is nulled, which re-reveals the Panel's email input so
  // the visitor can recover. Storing the failing STRING (not a boolean) means a
  // later identify() with a different address heals automatically.
  const [failedIdentityEmail, setFailedIdentityEmail] = useState<string | null>(
    null
  )
  const phaseRef = useRef(phase)
  phaseRef.current = phase
  const baseRef = useRef(base)
  baseRef.current = base

  const screenshot = annotated ?? base

  const accent =
    state.options.color ??
    state.config?.form?.accentColor ??
    theme.defaultAccent
  const position =
    state.options.position ?? state.config?.form?.position ?? `bottom-left`
  const label =
    state.options.label ?? state.config?.form?.buttonLabel ?? `Feedback`

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
    // A board whose config resolved disabled must never open — this is the
    // single gate that also covers the openRequested auto-open below when the
    // config disabled the widget before the bundle finished loading.
    if (state.disabled) return
    if (phaseRef.current.kind !== `closed`) return
    // Both modes enabled → the card home; a single mode skips it and opens
    // that form directly (feedback-only configs behave exactly like before).
    const modes = effectiveModes(state.config)
    setView(modes.length > 1 ? `home` : modes[0])
    // Screenshots are on demand: the feedback form opens plain and capturing
    // only happens when the reporter asks for it.
    setCaptureFailed(false)
    setPhase({ kind: `open` })
  }, [state])

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
  // and state.disabled by the time this continuation runs, so reading
  // state.disabled here is safe. A config that resolves the widget disabled
  // tears down any panel the reporter opened during the race; the
  // unconditional bump also guarantees a render that drops the FAB.
  useEffect(() => {
    let cancelled = false
    void state.configPromise.then(() => {
      if (cancelled) return
      if (state.disabled) close()
      bumpVersion((version) => version + 1)
    })
    return () => {
      cancelled = true
    }
  }, [state, close])

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

  // The usable identity email: the host-provided address unless the server
  // just refused it, in which case it's nulled so the Panel reveals its email
  // input and the submit paths fall back to the typed value instead.
  const identityEmail =
    state.identity.email && state.identity.email !== failedIdentityEmail
      ? state.identity.email
      : null

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
      // True when the submission relied on the hidden identity email (the
      // reporter typed none) — only then does an email rejection warrant
      // revealing the input.
      const usedIdentityEmail = !form.email && identityEmail !== null
      const result = await submitFeedback({
        state,
        title: form.title,
        description: form.description,
        email: form.email || identityEmail,
        screenshot: screenshotBlob,
        meta: collectEnvMeta(),
      })
      if (result.ok) {
        replaceBase(null)
        setCaptureFailed(false)
        setPhase({
          kind: `success`,
          flavor: `feedback`,
          identifier: result.identifier,
          url: result.url,
        })
        // Leave the success card up longer when it carries a link to the
        // public issue, so the reporter has a chance to click through.
        window.setTimeout(
          () => {
            setPhase((current) =>
              current.kind === `success` ? { kind: `closed` } : current
            )
          },
          result.url ? 6_000 : 2_500
        )
        return null
      }
      // Revealing the input is gated on usedIdentityEmail, so a
      // visitor-typed bad email just gets the friendlier message on the
      // already-visible field.
      if (usedIdentityEmail && isEmailFailure(result)) {
        setFailedIdentityEmail(identityEmail)
      }
      setPhase({ kind: `open` })
      return result.code === `invalid_email`
        ? `Please enter a valid email address.`
        : result.code === `email_required`
          ? `Your email is required.`
          : result.message
    },
    [state, screenshot, replaceBase, identityEmail]
  )

  const submitSupport = useCallback(
    async (form: { message: string; email: string }) => {
      setPhase({ kind: `submitting` })
      // Panel resolves email to identityEmail when hidden, else the typed
      // value — so a match (or empty) means the identity address was used.
      const usedIdentityEmail =
        identityEmail !== null &&
        (form.email === identityEmail || !form.email)
      const result = await submitSupportRequest({
        state,
        message: form.message,
        email: form.email || identityEmail || ``,
        meta: collectEnvMeta(),
      })
      if (result.ok) {
        setPhase({
          kind: `success`,
          flavor: `support`,
          identifier: null,
          url: null,
        })
        // Longer than the feedback flash: the card tells the reporter to
        // check their email for the conversation link.
        window.setTimeout(() => {
          setPhase((current) =>
            current.kind === `success` ? { kind: `closed` } : current
          )
        }, 6_000)
        return null
      }
      if (usedIdentityEmail && isEmailFailure(result)) {
        setFailedIdentityEmail(identityEmail)
      }
      setPhase({ kind: `open` })
      return result.code === `invalid_email`
        ? `Please enter a valid email address.`
        : result.code === `email_required`
          ? `Your email is required.`
          : result.message
    },
    [state, identityEmail]
  )

  const showButton =
    state.options.showButton !== false &&
    phase.kind !== `annotating` &&
    !state.disabled
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
          view={view}
          canGoBack={effectiveModes(state.config).length > 1}
          onPickMode={(mode) => setView(mode)}
          onBack={() => setView(`home`)}
          successFlavor={phase.kind === `success` ? phase.flavor : `feedback`}
          successIdentifier={phase.kind === `success` ? phase.identifier : null}
          successUrl={phase.kind === `success` ? phase.url : null}
          position={position}
          screenshot={screenshot}
          flattening={flattening}
          captureFailed={captureFailed}
          identityEmail={identityEmail}
          emailRequired={state.config?.form?.emailRequired === true}
          onClose={close}
          onCapture={takeScreenshot}
          onRetake={retake}
          onAnnotate={openAnnotator}
          onRemoveScreenshot={() => replaceBase(null)}
          onSubmit={submit}
          onSubmitSupport={submitSupport}
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
