import { useCallback, useEffect, useRef, useState } from "preact/hooks"
import type {
  WidgetMode,
  WidgetRemoteConfig,
  WidgetRuntimeState,
} from "../types"
import type { AnnotationShape, NormalizedRect } from "../annotate/shapes"
import { flattenAnnotations } from "../annotate/flatten"
import { captureScreenshot } from "../capture/engine"
import type { CaptureEngine } from "../capture/engine"
import { snapdomEngine } from "../capture/snapdom-engine"
import {
  displayMediaEngine,
  isDisplayCaptureSupported,
} from "../capture/display-media-engine"
import { collectEnvMeta } from "../env-meta"
import { submitFeedback, submitSupportRequest } from "../api-client"
import {
  megaphoneIconSvg,
  paletteFor,
  pickForeground,
  resolveThemeMode,
  resolveThemePreference,
} from "../theme"
import {
  isMobileViewport,
  launcherButtonClass,
  launcherOrigin,
  launcherPlacementCss,
  panelSideOffset,
  resolveLauncher,
  watchMobileViewport,
} from "../launcher"
import { Annotator } from "./Annotator"
import { ownCustomValue } from "./custom-values"
import { Panel, captureDelayCycle, type CaptureDelay } from "./Panel"
import type { PanelView } from "./Panel"
import {
  isAcceptedUploadImageType,
  maxUploadedImageBytes,
  maxUploadedImages,
} from "../uploads"

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
      // Support mode: whether the magic-link confirmation email went out
      // (null = not applicable / not reported).
      emailDelivered: boolean | null
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

// A reporter-attached picture (FEED-5) — dropped, pasted, or picked. Kept
// separate from the capture machinery: no annotation, just preview + remove.
export interface UploadedImage {
  id: string
  blob: Blob
  objectUrl: string
  filename: string
}

// Stable keys for the thumbnail list; a counter avoids a crypto dependency.
let nextUploadedImageId = 0

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
  // Delayed capture (FEED-18): session-only hold picked on the panel, and
  // the seconds left while it runs (null = no countdown showing). The ref
  // keeps `capture` stable across cycles.
  const [captureDelay, setCaptureDelay] = useState<CaptureDelay>(0)
  const captureDelayRef = useRef<CaptureDelay>(0)
  captureDelayRef.current = captureDelay
  const [countdown, setCountdown] = useState<number | null>(null)
  const cycleCaptureDelay = useCallback(
    () =>
      setCaptureDelay((current) => {
        const index = captureDelayCycle.indexOf(current)
        return captureDelayCycle[(index + 1) % captureDelayCycle.length]
      }),
    []
  )
  // Reporter-attached pictures (FEED-5), independent of the screenshot slot.
  const [uploads, setUploads] = useState<UploadedImage[]>([])
  const [uploadError, setUploadError] = useState<string | null>(null)
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
  const uploadsRef = useRef(uploads)
  uploadsRef.current = uploads

  const screenshot = annotated ?? base

  // Theme resolution (EXP-435): runtime setTheme > init option > widget
  // config > dark. `auto` re-resolves per render; the matchMedia effect
  // below forces one when the OS scheme flips.
  const themePref = resolveThemePreference(
    state.themeOverride,
    state.options.theme,
    state.config?.form?.theme
  )
  const palette = paletteFor(resolveThemeMode(themePref))
  const accent =
    state.options.color ??
    state.config?.form?.accentColor ??
    palette.defaultAccent
  const label =
    state.options.label ?? state.config?.form?.buttonLabel ?? `Feedback`

  // Per-device launcher appearance (EXP-569), re-resolved when the viewport
  // crosses the desktop/mobile breakpoint. Must agree with the loader's
  // resolution or the button jumps at bundle hand-off.
  const [isMobile, setIsMobile] = useState(isMobileViewport)
  useEffect(
    () => watchMobileViewport(() => setIsMobile(isMobileViewport())),
    []
  )
  const launcher = resolveLauncher(state.options, state.config, isMobile)

  // Live auto-switching: while the preference is `auto`, follow the OS
  // scheme. Guarded for environments without matchMedia (happy-dom tests).
  useEffect(() => {
    if (themePref !== `auto`) return
    if (typeof window.matchMedia !== `function`) return
    const query = window.matchMedia(`(prefers-color-scheme: light)`)
    const onChange = () => bumpVersion((version) => version + 1)
    if (typeof query.addEventListener === `function`) {
      query.addEventListener(`change`, onChange)
      return () => query.removeEventListener(`change`, onChange)
    }
    // Legacy Safari (pre-14) MediaQueryList.
    query.addListener(onChange)
    return () => query.removeListener(onChange)
  }, [themePref])

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

  // The engine behind the current shot, so Retake reproduces it the same way
  // (a display capture retakes via the picker, not a DOM raster).
  const lastEngineRef = useRef<CaptureEngine>(snapdomEngine)

  const capture = useCallback(
    async (engine: CaptureEngine): Promise<boolean> => {
      lastEngineRef.current = engine
      // Whether the hold actually ran. The display-media engine awaits it
      // only after the grant, so a dismissed picker never reaches it — the
      // first tick is the only reliable evidence either way.
      let held = false
      const options = {
        delayMs: captureDelayRef.current * 1000,
        onCountdown: (secondsLeft: number) => {
          held = true
          setCountdown(secondsLeft > 0 ? secondsLeft : null)
        },
      }
      let blob = await captureScreenshot(engine, options)
      if (!blob && engine === displayMediaEngine) {
        // A denied/cancelled share picker must not strand the reporter — the
        // DOM raster needs no user activation, so it can still run here. The
        // hold still applies to a dismissed picker (it never reached it), but
        // a grant that failed AFTER the hold must not make the reporter sit
        // through the countdown a second time.
        lastEngineRef.current = snapdomEngine
        blob = await captureScreenshot(
          snapdomEngine,
          held ? { ...options, delayMs: 0 } : options
        )
      }
      setCountdown(null)
      if (blob) {
        replaceBase({ blob, objectUrl: URL.createObjectURL(blob) })
        setCaptureFailed(false)
        return true
      }
      replaceBase(null)
      setCaptureFailed(true)
      return false
    },
    [replaceBase]
  )

  // Attach picked/dropped/pasted files, skipping anything that isn't an
  // accepted image or is over the per-file cap. The last rejection reason (or
  // the count cap) surfaces as the upload error; a clean add clears it.
  const addImages = useCallback((files: File[]) => {
    let error: string | null = null
    const accepted: UploadedImage[] = []
    for (const file of files) {
      if (!isAcceptedUploadImageType(file.type)) {
        error = `Only image files can be attached.`
        continue
      }
      if (file.size > maxUploadedImageBytes) {
        error = `Images must be 10 MB or smaller.`
        continue
      }
      if (uploadsRef.current.length + accepted.length >= maxUploadedImages) {
        error = `You can attach up to ${maxUploadedImages} images.`
        break
      }
      accepted.push({
        id: `upload-${++nextUploadedImageId}`,
        blob: file,
        objectUrl: URL.createObjectURL(file),
        filename: file.name || `image`,
      })
    }
    if (accepted.length > 0) {
      setUploads([...uploadsRef.current, ...accepted])
    }
    setUploadError(error)
  }, [])

  const removeUpload = useCallback((id: string) => {
    setUploads((previous) => {
      const target = previous.find((upload) => upload.id === id)
      if (target) URL.revokeObjectURL(target.objectUrl)
      return previous.filter((upload) => upload.id !== id)
    })
    setUploadError(null)
  }, [])

  const clearUploads = useCallback(() => {
    setUploads((previous) => {
      for (const upload of previous) URL.revokeObjectURL(upload.objectUrl)
      return []
    })
    setUploadError(null)
  }, [])

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
    // would be inconsistent, so it goes too — attached pictures likewise.
    replaceBase(null)
    clearUploads()
    setCaptureFailed(false)
    setPhase({ kind: `closed` })
  }, [replaceBase, clearUploads])

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

  // A close() — or a config that resolved the widget disabled — during a
  // multi-second hold (FEED-18) unmounts the panel while the capture is still
  // running. The shot landing afterwards must not reopen the panel or drop
  // the reporter into the annotator; it is discarded the way close() discards
  // the screenshot it had.
  const captureStillWanted = useCallback(() => {
    if (phaseRef.current.kind === `capturing`) return true
    replaceBase(null)
    setCaptureFailed(false)
    return false
  }, [replaceBase])

  const retake = useCallback(() => {
    // Close the panel, recapture (with whatever engine took the current
    // shot) without it, reopen.
    setPhase({ kind: `capturing` })
    requestAnimationFrame(() => {
      void capture(lastEngineRef.current).then(() => {
        if (!captureStillWanted()) return
        setPhase({ kind: `open` })
      })
    })
  }, [capture, captureStillWanted])

  const takeScreenshotWith = useCallback(
    (engine: CaptureEngine) => {
      // Hide the panel, capture the page as the reporter sees it, then land
      // in the annotation editor — capturing is an explicit intent to mark
      // up. One rAF only: getDisplayMedia must stay within the click's
      // transient user activation.
      setPhase({ kind: `capturing` })
      requestAnimationFrame(() => {
        void capture(engine).then((captured) => {
          if (!captureStillWanted()) return
          freshCaptureRef.current = captured
          setPhase(captured ? { kind: `annotating` } : { kind: `open` })
        })
      })
    },
    [capture, captureStillWanted]
  )

  // One button, engine picked by capability (EXP-488): native display capture
  // where the browser has it (desktop), the snapDOM raster otherwise (mobile).
  // Support is checked at click time so tests can stub navigator per case.
  const takeScreenshot = useCallback(
    () =>
      takeScreenshotWith(
        isDisplayCaptureSupported() ? displayMediaEngine : snapdomEngine
      ),
    [takeScreenshotWith]
  )

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

  const identityName = state.identity.name || null
  const remoteForm = state.config?.form
  // Absent on older servers = legacy behavior (email shown, no name field).
  // The email input must stay reachable while recovering from a rejected
  // identity email even when the owner hid it — otherwise invalid_email
  // would be a dead end.
  const collectEmail =
    remoteForm?.collectEmail !== false || failedIdentityEmail !== null
  const collectName = remoteForm?.collectName === true
  const nameRequired = collectName && remoteForm?.nameRequired === true
  // Server-sanitized, but old/self-hosted servers may relay junk — keep the
  // render path safe with a shape filter.
  const customFields = (
    Array.isArray(remoteForm?.customFields) ? remoteForm.customFields : []
  ).filter(
    (field) =>
      field !== null &&
      typeof field === `object` &&
      typeof field.key === `string` &&
      field.key.length > 0 &&
      typeof field.label === `string` &&
      field.label.length > 0
  )
  // Reporter-toggleable labels (EXP-435), same defensive shape filter; a
  // junk color falls back to the label default the web app uses.
  const labels = (Array.isArray(remoteForm?.labels) ? remoteForm.labels : [])
    .filter(
      (label) =>
        label !== null &&
        typeof label === `object` &&
        typeof label.id === `string` &&
        label.id.length > 0 &&
        typeof label.name === `string` &&
        label.name.length > 0
    )
    .slice(0, 10)
    .map((label) => ({
      ...label,
      color: /^#[0-9a-fA-F]{6}$/.test(label.color ?? ``)
        ? label.color
        : `#a1a1aa`,
    }))

  // The submission's customData: typed field values merged over the host's
  // setCustomData blob (a visible input the reporter filled wins over a
  // host-set key). Empty inputs don't overwrite host values.
  const mergeCustomValues = useCallback(
    (customValues: Record<string, string>) => {
      const merged = { ...state.customData }
      for (const field of customFields) {
        const value = ownCustomValue(customValues, field.key).trim()
        if (value) merged[field.key] = value
      }
      return merged
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [state, state.config]
  )

  const submit = useCallback(
    async (form: {
      title: string
      description: string
      email: string
      name: string
      customValues: Record<string, string>
      labelIds: string[]
      website: string
    }) => {
      // Mirrors the loader's setCustomData cap: the server rejects an
      // oversized blob with an uncoded 400, so fail with a clear message
      // before the network round-trip.
      const mergedCustomData = mergeCustomValues(form.customValues)
      if (JSON.stringify(mergedCustomData).length > 8 * 1024) {
        return `Your responses are too long to submit. Please shorten them.`
      }
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
        name: form.name || identityName,
        customData: mergedCustomData,
        screenshot: screenshotBlob,
        images: uploads.map((upload) => ({
          blob: upload.blob,
          filename: upload.filename,
        })),
        labelIds: form.labelIds,
        website: form.website,
        meta: collectEnvMeta(),
      })
      if (result.ok) {
        replaceBase(null)
        clearUploads()
        setCaptureFailed(false)
        setPhase({
          kind: `success`,
          flavor: `feedback`,
          identifier: result.identifier,
          url: result.url,
          emailDelivered: null,
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
          : result.code === `name_required`
            ? `Your name is required.`
            : result.message
    },
    [
      state,
      screenshot,
      uploads,
      replaceBase,
      clearUploads,
      identityEmail,
      identityName,
      mergeCustomValues,
    ]
  )

  const submitSupport = useCallback(
    async (form: {
      message: string
      email: string
      name: string
      website: string
    }) => {
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
        name: form.name || identityName,
        website: form.website,
        meta: collectEnvMeta(),
      })
      if (result.ok) {
        setPhase({
          kind: `success`,
          flavor: `support`,
          identifier: null,
          url: null,
          emailDelivered: result.emailDelivered ?? null,
        })
        // Longer than the feedback flash: the card tells the reporter to
        // check their email for the conversation link — longer still when it
        // has to explain that the email did NOT arrive.
        window.setTimeout(
          () => {
            setPhase((current) =>
              current.kind === `success` ? { kind: `closed` } : current
            )
          },
          result.emailDelivered === false ? 10_000 : 6_000
        )
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
          : result.code === `name_required`
            ? `Your name is required.`
            : result.message
    },
    [state, identityEmail, identityName]
  )

  // Hidden while capturing too (EXP-435): a display-media frame can't
  // exclude the FAB by selector the way the snapDOM clone does.
  // setLauncherHidden() (EXP-642) only takes the button away — the panel
  // below keeps rendering, so a host hiding the launcher while its own sheet
  // is up can't close a panel the visitor is typing in.
  const showButton =
    state.options.showButton !== false &&
    phase.kind !== `annotating` &&
    phase.kind !== `capturing` &&
    state.launcherHidden !== true &&
    !state.disabled
  const panelVisible =
    phase.kind === `open` ||
    phase.kind === `submitting` ||
    phase.kind === `success`
  // Keep the Panel mounted (display:none) while annotating and capturing so
  // the typed title/description survive the round-trip into the editor and
  // a delayed capture's multi-second hold (FEED-18).
  const panelHidden =
    phase.kind === `annotating` || phase.kind === `capturing`
  const panelMounted = panelVisible || panelHidden

  const rootRef = useRef<HTMLDivElement>(null)

  // Mobile on-screen keyboards shrink only the VISUAL viewport — the layout
  // viewport that `position: fixed` anchors to keeps its height, so the
  // panel's lower half (email field, submit button) ended up hidden behind
  // the Android keyboard. While the panel is mounted, mirror the visual
  // viewport into CSS vars the panel geometry in widget.css reads.
  useEffect(() => {
    const root = rootRef.current
    const viewport = window.visualViewport
    if (!panelMounted || !root || !viewport) return
    const clear = () => {
      root.style.removeProperty(`--exp-vv-height`)
      root.style.removeProperty(`--exp-vv-inset`)
    }
    const apply = () => {
      // Pinch-zoom shrinks the visual viewport too; only compensate for
      // unzoomed insets (the keyboard) so zoomed pages keep native behavior.
      if (Math.abs(viewport.scale - 1) > 0.01) {
        clear()
        return
      }
      const inset = Math.max(
        0,
        window.innerHeight - viewport.height - viewport.offsetTop
      )
      root.style.setProperty(`--exp-vv-height`, `${viewport.height}px`)
      root.style.setProperty(`--exp-vv-inset`, `${inset}px`)
    }
    apply()
    viewport.addEventListener(`resize`, apply)
    viewport.addEventListener(`scroll`, apply)
    return () => {
      viewport.removeEventListener(`resize`, apply)
      viewport.removeEventListener(`scroll`, apply)
      clear()
    }
  }, [panelMounted])

  return (
    <div
      ref={rootRef}
      className="exp-root"
      style={{
        "--exp-font": palette.font,
        "--exp-background": palette.background,
        "--exp-card": palette.card,
        "--exp-secondary": palette.secondary,
        "--exp-foreground": palette.foreground,
        "--exp-muted-foreground": palette.mutedForeground,
        "--exp-border": palette.border,
        "--exp-input": palette.input,
        "--exp-destructive": palette.destructive,
        "--exp-success": palette.success,
        "--exp-radius": palette.radius,
        "--exp-accent": accent,
        "--exp-accent-foreground": pickForeground(accent),
        "--exp-panel-side": panelSideOffset(launcher),
      }}
    >
      {showButton && (
        // The wrapper carries placement (incl. middle centering), the button
        // carries hover transforms — the same split as the loader's render,
        // via the same launcherPlacementCss string.
        <div style={`position:fixed;${launcherPlacementCss(launcher)}`}>
          <button
            className={launcherButtonClass(launcher)}
            style={
              launcher.mode === `fab`
                ? { transformOrigin: launcherOrigin(launcher.position) }
                : undefined
            }
            aria-label="Send feedback"
            aria-haspopup="dialog"
            aria-expanded={panelVisible}
            onClick={() => (panelVisible ? close() : open())}
          >
            <span
              style={{ display: `flex` }}
              // eslint-disable-next-line react/no-danger
              dangerouslySetInnerHTML={{
                __html: launcher.iconSvg ?? megaphoneIconSvg,
              }}
            />
            {launcher.mode === `fab` && label ? (
              <span className="exp-fab-label">{label}</span>
            ) : null}
          </button>
        </div>
      )}

      {countdown !== null && (
        // Delayed capture (FEED-18): the seconds left, in the launcher's
        // spot (the FAB is hidden while capturing). runCountdown clears it
        // and lets a frame paint before the engine grabs the shot, so it is
        // never in the picture.
        <div style={`position:fixed;${launcherPlacementCss(launcher)}`}>
          <div className="exp-countdown" role="status" aria-live="polite">
            {countdown}
          </div>
        </div>
      )}

      {panelMounted && (
        <Panel
          phase={panelHidden ? `open` : phase.kind}
          hidden={panelHidden}
          captureDelay={captureDelay}
          onCycleCaptureDelay={cycleCaptureDelay}
          view={view}
          canGoBack={effectiveModes(state.config).length > 1}
          onPickMode={(mode) => setView(mode)}
          onBack={() => setView(`home`)}
          successFlavor={phase.kind === `success` ? phase.flavor : `feedback`}
          successIdentifier={phase.kind === `success` ? phase.identifier : null}
          successUrl={phase.kind === `success` ? phase.url : null}
          successEmailDelivered={
            phase.kind === `success` ? phase.emailDelivered : null
          }
          position={launcher.position}
          screenshot={screenshot}
          flattening={flattening}
          captureFailed={captureFailed}
          uploads={uploads}
          uploadError={uploadError}
          identityEmail={identityEmail}
          emailRequired={state.config?.form?.emailRequired === true}
          collectEmail={collectEmail}
          identityName={identityName}
          collectName={collectName}
          nameRequired={nameRequired}
          customFields={customFields}
          labels={labels}
          onClose={close}
          onCapture={takeScreenshot}
          onRetake={retake}
          onAnnotate={openAnnotator}
          onRemoveScreenshot={() => replaceBase(null)}
          onAddImages={addImages}
          onRemoveUpload={removeUpload}
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
