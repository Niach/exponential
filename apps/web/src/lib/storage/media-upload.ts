// EXP-824 — client side of inline video/audio uploads.
//
// Before a clip leaves the browser we read what the server may not be able to
// probe itself (webm has no MP4 header), grab a poster frame, and losslessly
// remux QuickTime containers into MP4 so Safari-recorded `.mov` files play in
// every `<video>` element. All of it rides on Mediabunny (MPL-2.0, in the
// licence inventory) loaded through ONE memoised dynamic import — the chunk
// is ~200 KB and only a media upload ever needs it (the `loadEmojiData`
// pattern). Every probe step is best-effort: a decode failure degrades to
// "upload the file as-is", never to a blocked upload.

import type { UploadedIssueAttachment } from "@/lib/storage/issue-image-upload"
import {
  isAudioContentType,
  isVideoContentType,
  maxFileUploadBytes,
  maxPosterUploadBytes,
} from "@/lib/storage/issue-attachments"

type Mediabunny = typeof import("mediabunny")

let mediabunny: Promise<Mediabunny> | null = null

/** Loads Mediabunny once; a failed chunk load is retried on the next call. */
export function loadMediabunny(): Promise<Mediabunny> {
  if (!mediabunny) {
    mediabunny = import(`mediabunny`)
    mediabunny.catch(() => {
      mediabunny = null
    })
  }
  return mediabunny
}

/** The `/files` route's response for a media upload (superset of images). */
export interface UploadedIssueMediaAttachment extends UploadedIssueAttachment {
  durationMs: number | null
  posterUrl: string | null
  /** Sample-entry fourcc of the first video track (`avc1`, `hvc1`, `av01`…). */
  videoCodec: string | null
  audioCodec: string | null
}

export interface PreparedMediaUpload {
  /** The bytes to send — the original file, or the `.mp4` remux of a `.mov`. */
  file: File
  poster: Blob | null
  width: number | null
  height: number | null
  durationMs: number | null
  /** True when the container was rewritten (label/filename end in `.mp4`). */
  remuxed: boolean
}

export type MediaPrepareStage = `analyzing` | `remuxing`

// Poster frames are display stills, never the source resolution: 1280px wide
// keeps a 4K clip's poster well under the server's 2 MB ceiling.
const maxPosterWidth = 1280
const posterJpegQuality = 0.82
// A camera's first frame is often black/blurry; a short skip lands on a real
// picture without visibly disagreeing with what the player shows at t=0.
const posterOffsetSeconds = 0.1
// The `<video>`-element poster fallback needs a bounded wait — a codec the
// browser cannot decode never fires `loadeddata`.
const posterFallbackTimeoutMs = 8000

function canvasToJpeg(canvas: HTMLCanvasElement | OffscreenCanvas) {
  if (`convertToBlob` in canvas) {
    return canvas.convertToBlob({ type: `image/jpeg`, quality: posterJpegQuality })
  }
  return new Promise<Blob>((resolve, reject) => {
    canvas.toBlob(
      (blob) => (blob ? resolve(blob) : reject(new Error(`poster encode failed`))),
      `image/jpeg`,
      posterJpegQuality
    )
  })
}

/**
 * Poster via the browser's own decoder: seek a detached `<video>` and draw
 * the frame. Used when WebCodecs cannot decode the track (or is absent).
 */
async function grabPosterWithVideoElement(file: File): Promise<Blob | null> {
  if (typeof document === `undefined`) return null
  const objectUrl = URL.createObjectURL(file)
  const video = document.createElement(`video`)
  video.muted = true
  video.playsInline = true
  video.preload = `auto`
  video.src = objectUrl
  try {
    await new Promise<void>((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error(`poster timeout`)), posterFallbackTimeoutMs)
      const done = () => {
        clearTimeout(timer)
        resolve()
      }
      video.addEventListener(`error`, () => {
        clearTimeout(timer)
        reject(new Error(`poster decode failed`))
      })
      video.addEventListener(
        `loadedmetadata`,
        () => {
          const target = Math.min(posterOffsetSeconds, Math.max(0, video.duration / 2))
          video.addEventListener(`seeked`, done, { once: true })
          video.currentTime = target
        },
        { once: true }
      )
    })
    if (!video.videoWidth || !video.videoHeight) return null
    const scale = Math.min(1, maxPosterWidth / video.videoWidth)
    const canvas = document.createElement(`canvas`)
    canvas.width = Math.round(video.videoWidth * scale)
    canvas.height = Math.round(video.videoHeight * scale)
    const context = canvas.getContext(`2d`)
    if (!context) return null
    context.drawImage(video, 0, 0, canvas.width, canvas.height)
    return await canvasToJpeg(canvas)
  } catch {
    return null
  } finally {
    video.removeAttribute(`src`)
    video.load()
    URL.revokeObjectURL(objectUrl)
  }
}

function isQuickTime(file: File) {
  return file.type === `video/quicktime` || /\.mov$/i.test(file.name)
}

/**
 * Probe + poster + remux. Never throws: whatever step fails, the caller gets
 * a file to upload and nulls for the fields the server will fill from its
 * own header probe (or leave empty).
 */
export async function prepareMediaUpload(
  file: File,
  onStage?: (stage: MediaPrepareStage) => void
): Promise<PreparedMediaUpload> {
  const fallback: PreparedMediaUpload = {
    file,
    poster: null,
    width: null,
    height: null,
    durationMs: null,
    remuxed: false,
  }
  if (!isVideoContentType(file.type) && !isAudioContentType(file.type)) {
    return fallback
  }

  let mb: Mediabunny
  try {
    mb = await loadMediabunny()
  } catch {
    return fallback
  }

  onStage?.(`analyzing`)
  const input = new mb.Input({
    source: new mb.BlobSource(file),
    formats: mb.ALL_FORMATS,
  })
  try {
    if (!(await input.canRead())) return fallback

    const prepared: PreparedMediaUpload = { ...fallback }
    try {
      const seconds = await input.computeDuration()
      if (Number.isFinite(seconds) && seconds > 0) {
        prepared.durationMs = Math.round(seconds * 1000)
      }
    } catch {
      // Duration stays null — the server probe may still know it.
    }

    const videoTrack = isVideoContentType(file.type)
      ? await input.getPrimaryVideoTrack().catch(() => null)
      : null
    if (!videoTrack) return prepared

    // Display dimensions are rotation-aware: a portrait phone clip reports
    // portrait numbers, matching what the server's tkhd matrix read yields.
    if (videoTrack.displayWidth > 0 && videoTrack.displayHeight > 0) {
      prepared.width = videoTrack.displayWidth
      prepared.height = videoTrack.displayHeight
    }

    // Poster: WebCodecs through Mediabunny's CanvasSink when the track is
    // decodable here, else the `<video>` element; silently none otherwise.
    try {
      if (await videoTrack.canDecode()) {
        const sink = new mb.CanvasSink(videoTrack, {
          width: Math.min(maxPosterWidth, videoTrack.displayWidth || maxPosterWidth),
          fit: `contain`,
        })
        const first = await input.getFirstTimestamp()
        const wrapped =
          (await sink.getCanvas(first + posterOffsetSeconds)) ??
          (await sink.getCanvas(first))
        if (wrapped) prepared.poster = await canvasToJpeg(wrapped.canvas)
      }
    } catch {
      prepared.poster = null
    }
    if (!prepared.poster) {
      prepared.poster = await grabPosterWithVideoElement(file)
    }
    if (prepared.poster && prepared.poster.size > maxPosterUploadBytes) {
      prepared.poster = null
    }

    // `.mov` → `.mp4` without re-encoding: only when every track copies
    // (avc/hevc video, aac audio). Anything Mediabunny would have to
    // transcode or drop keeps the original container.
    if (isQuickTime(file)) {
      const audioTrack = await input.getPrimaryAudioTrack().catch(() => null)
      const videoCopyable =
        videoTrack.codec === `avc` || videoTrack.codec === `hevc`
      const audioCopyable = !audioTrack || audioTrack.codec === `aac`
      if (videoCopyable && audioCopyable) {
        onStage?.(`remuxing`)
        try {
          const target = new mb.BufferTarget()
          const output = new mb.Output({
            format: new mb.Mp4OutputFormat({ fastStart: `in-memory` }),
            target,
          })
          const conversion = await mb.Conversion.init({
            input,
            output,
            copy: { mode: `forced` },
            showWarnings: false,
          })
          if (conversion.isValid && conversion.discardedTracks.length === 0) {
            await conversion.execute()
            if (target.buffer && target.buffer.byteLength <= maxFileUploadBytes) {
              prepared.file = new File(
                [target.buffer],
                file.name.replace(/\.mov$/i, ``) + `.mp4`,
                { type: `video/mp4`, lastModified: file.lastModified }
              )
              prepared.remuxed = true
            }
          }
        } catch {
          prepared.file = file
          prepared.remuxed = false
        }
      }
    }

    return prepared
  } catch {
    return fallback
  } finally {
    input.dispose()
  }
}

export interface MediaUploadOptions {
  /** Upload progress in whole percent (0–100), from the XHR upload stream. */
  onProgress?: (percent: number) => void
  signal?: AbortSignal
}

/**
 * Multipart POST of a prepared clip to `/api/issues/{issueId}/files`: `file`
 * plus the optional `poster` part and the probed `width`/`height`/`durationMs`
 * fields (the server's own MP4/MOV probe wins when it succeeds; these fill in
 * for webm). XHR rather than fetch so a 50 MB upload can report progress.
 */
export function uploadIssueMediaFile(
  issueId: string,
  prepared: PreparedMediaUpload,
  options: MediaUploadOptions = {}
): Promise<UploadedIssueMediaAttachment> {
  const formData = new FormData()
  formData.append(`file`, prepared.file)
  if (prepared.poster) {
    formData.append(`poster`, prepared.poster, `poster.jpg`)
  }
  if (prepared.width && prepared.height) {
    formData.append(`width`, String(prepared.width))
    formData.append(`height`, String(prepared.height))
  }
  if (prepared.durationMs && prepared.durationMs > 0) {
    formData.append(`durationMs`, String(prepared.durationMs))
  }

  return new Promise((resolve, reject) => {
    const xhr = new XMLHttpRequest()
    xhr.open(`POST`, `/api/issues/${issueId}/files`)
    xhr.withCredentials = true
    xhr.responseType = `json`
    xhr.upload.onprogress = (event) => {
      if (!event.lengthComputable) return
      options.onProgress?.(
        Math.max(0, Math.min(100, Math.round((event.loaded / event.total) * 100)))
      )
    }
    xhr.onerror = () => reject(new Error(`Failed to upload ${prepared.file.name}`))
    xhr.onabort = () => reject(new Error(`Upload cancelled`))
    xhr.onload = () => {
      const result = (xhr.response ?? null) as
        | { error?: string }
        | UploadedIssueMediaAttachment
        | null
      if (xhr.status >= 200 && xhr.status < 300 && result && `url` in result) {
        resolve(result)
        return
      }
      reject(
        new Error(
          result && `error` in result && typeof result.error === `string`
            ? result.error
            : `Failed to upload ${prepared.file.name}`
        )
      )
    }
    options.signal?.addEventListener(`abort`, () => xhr.abort(), { once: true })
    xhr.send(formData)
  })
}

// The guaranteed-playable target is H.264 + AAC in MP4 (EXP-775). `avc1` and
// `avc3` are both H.264 sample entries.
const universallyPlayableVideoCodecs = new Set([`avc1`, `avc3`])

/**
 * Non-blocking hint for a clip that uploaded fine but may not decode on every
 * device: HEVC/AV1 in MP4, anything in webm. Null when nothing to say.
 */
export function mediaPlayabilityHint(
  uploaded: Pick<UploadedIssueMediaAttachment, `contentType` | `filename` | `videoCodec`>
): string | null {
  if (!isVideoContentType(uploaded.contentType)) return null
  const webm = uploaded.contentType === `video/webm`
  const nonH264 =
    uploaded.videoCodec !== null &&
    !universallyPlayableVideoCodecs.has(uploaded.videoCodec.toLowerCase())
  if (!webm && !nonH264) return null
  return `${uploaded.filename} may not play on every device. H.264 MP4 plays everywhere.`
}

/** The media block markdown form — a plain link on its own paragraph. */
export function buildMediaBlockMarkdown(label: string, url: string) {
  const escapedLabel = label.replace(/[[\]\\]/g, `\\$&`)
  const escapedUrl = url.replace(/[()]/g, `\\$&`)
  return `[${escapedLabel}](${escapedUrl})`
}

/** Appends media blocks (each its own paragraph) to a markdown body. */
export function appendMediaBlocks(markdown: string, blocks: string[]) {
  if (blocks.length === 0) return markdown
  const body = markdown.replace(/\s+$/, ``)
  const tail = blocks.join(`\n\n`)
  return body.length > 0 ? `${body}\n\n${tail}` : tail
}
