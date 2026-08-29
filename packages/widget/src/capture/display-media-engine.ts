import type { CaptureEngine } from "./engine"

// Native display capture (EXP-435): getDisplayMedia grabs one frame of
// whatever surface the visitor picks — the escape hatch for everything the
// DOM-cloning snapDOM engine cannot rasterize (tainted canvases, cross-origin
// iframes, video, WebGL). Desktop-browser only; callers feature-detect with
// isDisplayCaptureSupported() before offering it.

export function isDisplayCaptureSupported(): boolean {
  return (
    typeof navigator !== `undefined` &&
    typeof navigator.mediaDevices?.getDisplayMedia === `function`
  )
}

// Chromium-only picker hints (safely ignored elsewhere): default the picker
// to this tab, let it offer the tab at all, drop the share-instead controls
// and the screen option's noisier variants.
interface DisplayMediaHints {
  preferCurrentTab?: boolean
  selfBrowserSurface?: `include` | `exclude`
  surfaceSwitching?: `include` | `exclude`
  monitorTypeSurfaces?: `include` | `exclude`
}

function waitForFrame(video: HTMLVideoElement): Promise<void> {
  return new Promise((resolve) => {
    const withFrameCallback = video as HTMLVideoElement & {
      requestVideoFrameCallback?: (callback: () => void) => void
    }
    if (typeof withFrameCallback.requestVideoFrameCallback === `function`) {
      withFrameCallback.requestVideoFrameCallback(() => resolve())
      return
    }
    // Fallback: loadeddata plus a short settle so the browser's share
    // bar/picker overlay has faded out of the frame.
    const settle = () => window.setTimeout(resolve, 150)
    if (video.readyState >= 2) {
      settle()
      return
    }
    video.addEventListener(`loadeddata`, settle, { once: true })
  })
}

export const displayMediaEngine: CaptureEngine = {
  name: `display-media`,
  // The frame is already the picked surface — no document-space crop.
  precropped: true,
  // The surface picker is an interactive browser dialog; the DOM engines'
  // 6s budget would kill every real capture.
  captureTimeoutMs: 60_000,
  // excludeSelectors/keepNode are accepted for interface compliance but
  // cannot apply — a display frame has no DOM to filter. The caller hides
  // the widget's own UI (FAB included) before invoking.
  async capture({ beforeFrame }): Promise<HTMLCanvasElement> {
    const constraints: MediaStreamConstraints & DisplayMediaHints = {
      video: true,
      audio: false,
      preferCurrentTab: true,
      selfBrowserSurface: `include`,
      surfaceSwitching: `exclude`,
      monitorTypeSurfaces: `exclude`,
    }
    const stream = await navigator.mediaDevices.getDisplayMedia(constraints)
    const video = document.createElement(`video`)
    try {
      video.muted = true
      video.playsInline = true
      video.srcObject = stream
      await video.play()
      await waitForFrame(video)
      // A delayed capture (FEED-18) waits HERE, with the stream live: the
      // picker consumed the click's activation, so the delay can only run
      // after the grant. The reporter opens their popup during this hold.
      await beforeFrame()
      // The reporter can end the share during that hold (the browser's "Stop
      // sharing" bar): the track is dead and drawImage would paint a blank
      // frame, so fail instead and let the caller's snapDOM fallback run.
      if (!stream.active) {
        throw new Error(`display capture stream ended during the hold`)
      }

      const canvas = document.createElement(`canvas`)
      canvas.width = video.videoWidth
      canvas.height = video.videoHeight
      const context = canvas.getContext(`2d`)
      if (!context || canvas.width === 0 || canvas.height === 0) {
        throw new Error(`display capture produced no frame`)
      }
      context.drawImage(video, 0, 0)
      return canvas
    } finally {
      // One frame is all we take — never leave the capture indicator on.
      for (const track of stream.getTracks()) track.stop()
      video.srcObject = null
    }
  },
}
