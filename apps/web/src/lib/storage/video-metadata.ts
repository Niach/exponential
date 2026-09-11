// Dependency-free MP4 / QuickTime header probe (EXP-824), the video twin of
// image-dimensions.ts. Walks the ISO-BMFF box tree to `moov` — which a camera
// recording puts at the END of the file, after `mdat` — reads `mvhd` for the
// duration, the first video `tkhd` for the display size (rotation matrix
// applied, so a portrait phone clip reports portrait dimensions) and the
// sample entries for the codec fourccs. WebM and anything unrecognised return
// null: callers treat the result as best-effort and never block an upload.

export interface VideoMetadata {
  width: number | null
  height: number | null
  durationMs: number | null
  /** Sample-entry fourcc of the first video track (`avc1`, `hvc1`, `av01`…). */
  videoCodec: string | null
  /** Sample-entry fourcc of the first audio track (`mp4a`, `Opus`, `ac-3`…). */
  audioCodec: string | null
}

// Same reasoning as MAX_PLAUSIBLE_DIMENSION in image-dimensions.ts: these are
// attacker-controlled bytes headed for int4 columns. A declared width past
// 65535 or a duration past two days is not a clip we could serve — degrade to
// null rather than abort the insert.
const MAX_PLAUSIBLE_DIMENSION = 65535
const MAX_PLAUSIBLE_DURATION_MS = 48 * 60 * 60 * 1000
// The walker is not recursive (every box we read sits at a fixed depth), so
// the only unbounded loop is the track list — a clip has one or two tracks.
const MAX_TRACKS = 64
// A single-frame black poster clip is ~1 KB; anything shorter than one box
// header plus an `ftyp` payload can't be a container.
const MIN_FILE_BYTES = 16

const TOP_LEVEL_BOXES = new Set([
  `ftyp`,
  `moov`,
  `mdat`,
  `free`,
  `skip`,
  `wide`,
  `pdin`,
  `uuid`,
  `moof`,
  `mfra`,
  `meta`,
  `sidx`,
  `styp`,
])

/** Content types the header probe understands (webm is Matroska, not BMFF). */
export function isProbeableVideoContentType(contentType: string) {
  return (
    contentType === `video/mp4` ||
    contentType === `video/quicktime` ||
    contentType === `video/x-m4v` ||
    contentType === `audio/mp4` ||
    contentType === `audio/x-m4a`
  )
}

interface Box {
  type: string
  /** Offset of the first payload byte (after the header). */
  start: number
  /** Offset one past the last payload byte. */
  end: number
}

function fourcc(b: Uint8Array, offset: number) {
  return String.fromCharCode(
    b[offset]!,
    b[offset + 1]!,
    b[offset + 2]!,
    b[offset + 3]!
  )
}

function readU32(view: DataView, offset: number) {
  return view.getUint32(offset)
}

function readU64(view: DataView, offset: number) {
  // Sizes and durations past 2^53 are not real; Number() is the bound.
  return Number(view.getBigUint64(offset))
}

/** Iterates the boxes in [start, end); stops at the first malformed header. */
function* boxes(
  b: Uint8Array,
  view: DataView,
  start: number,
  end: number
): Generator<Box> {
  let offset = start
  while (offset + 8 <= end) {
    let size = readU32(view, offset)
    const type = fourcc(b, offset + 4)
    let headerLength = 8
    if (size === 1) {
      if (offset + 16 > end) return
      size = readU64(view, offset + 8)
      headerLength = 16
    } else if (size === 0) {
      // "Extends to the end of the file" — only legal for the last top-level
      // box (typically mdat).
      size = end - offset
    }
    if (size < headerLength || offset + size > end) return
    yield { type, start: offset + headerLength, end: offset + size }
    offset += size
  }
}

function findBox(
  b: Uint8Array,
  view: DataView,
  start: number,
  end: number,
  type: string
): Box | null {
  for (const box of boxes(b, view, start, end)) {
    if (box.type === type) return box
  }
  return null
}

function readMvhd(view: DataView, box: Box): number | null {
  const version = view.getUint8(box.start)
  let timescale: number
  let duration: number
  if (version === 1) {
    if (box.end - box.start < 4 + 8 + 8 + 4 + 8) return null
    timescale = readU32(view, box.start + 20)
    duration = readU64(view, box.start + 24)
  } else {
    if (box.end - box.start < 4 + 4 + 4 + 4 + 4) return null
    timescale = readU32(view, box.start + 12)
    duration = readU32(view, box.start + 16)
    // 0xFFFFFFFF = "unknown" in a 32-bit mvhd.
    if (duration === 0xffffffff) return null
  }
  if (timescale < 1 || !Number.isFinite(duration)) return null
  return Math.round((duration / timescale) * 1000)
}

interface TrackHeader {
  width: number
  height: number
}

function readTkhd(view: DataView, box: Box): TrackHeader | null {
  const version = view.getUint8(box.start)
  // fullbox header (4) + creation/modification/track_id/reserved/duration,
  // then reserved(8) layer(2) alternate_group(2) volume(2) reserved(2)
  // matrix(36) width(4) height(4).
  const fixedStart = version === 1 ? box.start + 4 + 8 + 8 + 4 + 4 + 8 : box.start + 4 + 4 + 4 + 4 + 4 + 4
  const matrixStart = fixedStart + 8 + 2 + 2 + 2 + 2
  const sizeStart = matrixStart + 36
  if (sizeStart + 8 > box.end) return null

  // 16.16 fixed point: the integer part is the high 16 bits.
  const width = readU32(view, sizeStart) >>> 16
  const height = readU32(view, sizeStart + 4) >>> 16

  // Rotation matrix [a b u; c d v; x y w] as 16.16 fixed (u/v/w are 2.30).
  // 90° / 270° clips have a = d = 0 with b and c non-zero: the display
  // dimensions swap. 0° / 180° keep them.
  const a = view.getInt32(matrixStart)
  const bb = view.getInt32(matrixStart + 4)
  const c = view.getInt32(matrixStart + 12)
  const d = view.getInt32(matrixStart + 16)
  const rotated = a === 0 && d === 0 && bb !== 0 && c !== 0

  return rotated ? { width: height, height: width } : { width, height }
}

function readHandlerType(b: Uint8Array, view: DataView, mdia: Box) {
  const hdlr = findBox(b, view, mdia.start, mdia.end, `hdlr`)
  // fullbox(4) + pre_defined(4) + handler_type(4)
  if (!hdlr || hdlr.end - hdlr.start < 12) return null
  return fourcc(b, hdlr.start + 8)
}

function readSampleEntryType(b: Uint8Array, view: DataView, mdia: Box) {
  const minf = findBox(b, view, mdia.start, mdia.end, `minf`)
  if (!minf) return null
  const stbl = findBox(b, view, minf.start, minf.end, `stbl`)
  if (!stbl) return null
  const stsd = findBox(b, view, stbl.start, stbl.end, `stsd`)
  // fullbox(4) + entry_count(4), then the first sample entry box.
  if (!stsd || stsd.end - stsd.start < 16) return null
  const entryCount = readU32(view, stsd.start + 4)
  if (entryCount < 1) return null
  for (const entry of boxes(b, view, stsd.start + 8, stsd.end)) {
    return entry.type
  }
  return null
}

function plausibleDimension(value: number) {
  return value >= 1 && value <= MAX_PLAUSIBLE_DIMENSION
}

/**
 * Probes an MP4/MOV file. Returns null when the bytes are not an ISO-BMFF
 * container; individual fields are null when their box is missing or carries
 * implausible values. Never throws.
 */
export function getVideoMetadata(bytes: Uint8Array): VideoMetadata | null {
  try {
    return probe(bytes)
  } catch {
    return null
  }
}

function probe(b: Uint8Array): VideoMetadata | null {
  if (b.length < MIN_FILE_BYTES) return null
  const view = new DataView(b.buffer, b.byteOffset, b.byteLength)

  // The first box must be a known top-level type — this is the whole "is it
  // a container at all" check, so a PNG or a Matroska file bails here.
  const firstSize = readU32(view, 0)
  const firstType = fourcc(b, 4)
  if ((firstSize < 8 && firstSize !== 0 && firstSize !== 1) || !TOP_LEVEL_BOXES.has(firstType)) {
    return null
  }

  const moov = findBox(b, view, 0, b.length, `moov`)
  if (!moov) return null

  const result: VideoMetadata = {
    width: null,
    height: null,
    durationMs: null,
    videoCodec: null,
    audioCodec: null,
  }

  const mvhd = findBox(b, view, moov.start, moov.end, `mvhd`)
  if (mvhd) {
    const durationMs = readMvhd(view, mvhd)
    if (
      durationMs !== null &&
      durationMs >= 0 &&
      durationMs <= MAX_PLAUSIBLE_DURATION_MS
    ) {
      result.durationMs = durationMs
    }
  }

  let tracks = 0
  for (const trak of boxes(b, view, moov.start, moov.end)) {
    if (trak.type !== `trak`) continue
    if (++tracks > MAX_TRACKS) break

    const mdia = findBox(b, view, trak.start, trak.end, `mdia`)
    const handler = mdia ? readHandlerType(b, view, mdia) : null
    const codec = mdia ? readSampleEntryType(b, view, mdia) : null

    if (handler === `soun`) {
      result.audioCodec ??= codec
      continue
    }

    // `vide` is the video handler; QuickTime exports occasionally omit hdlr,
    // so a track with a non-zero tkhd size counts as video too.
    const tkhd = findBox(b, view, trak.start, trak.end, `tkhd`)
    const header = tkhd ? readTkhd(view, tkhd) : null
    const hasSize =
      header !== null &&
      plausibleDimension(header.width) &&
      plausibleDimension(header.height)

    if (handler === `vide` || (handler === null && hasSize)) {
      if (result.videoCodec === null) result.videoCodec = codec
      if (result.width === null && hasSize) {
        result.width = header!.width
        result.height = header!.height
      }
    }
  }

  return result
}

/** Human duration for chips and rails: `0:07`, `2:34`, `1:02:03`. */
export function formatDuration(durationMs: number) {
  const totalSeconds = Math.max(0, Math.round(durationMs / 1000))
  const hours = Math.floor(totalSeconds / 3600)
  const minutes = Math.floor((totalSeconds % 3600) / 60)
  const seconds = totalSeconds % 60
  const two = (value: number) => value.toString().padStart(2, `0`)
  return hours > 0
    ? `${hours}:${two(minutes)}:${two(seconds)}`
    : `${minutes}:${two(seconds)}`
}
