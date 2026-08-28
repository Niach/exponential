// closedloop/surfaces/reviewphone.tsx — the mobile Review screen (FEED-20):
// the REAL iOS ChangesView (shots/review-diff/ios.webp) a Reviews row opens —
// "Review" nav bar, the glass summary card (branch · Open pill · files +/−),
// unified-diff file cards, and the floating bottom bar × · Merge · ↗ (EXP-248
// chrome). The merge runs the way it does on the phone: tap Merge → the
// "Merge pull request?" alert → Merge → spinner in the capsule → the summary
// pill flips to Merged and the bar collapses to the GitHub circle, exactly as
// the real bar does once the PR is no longer open.
//
// Every number is authored in iOS POINTS on the 414pt canvas and scaled ONCE
// through `pt()` (the startphone.tsx convention). All frame props are
// COMPOSITION-LOCAL to the segment that renders it.

import React from "react"
import { interpolate, spring } from "remotion"
import { C, EASE, MONO_FONT, SETTLE, UI_FONT } from "../../ships/theme"
import type { DiffRow } from "../../ships/fixtures"
import { CL, CL_DIFF_FILES, CL_DIFF_ROWS, CL_FILE_STATS, CL_PR_HEAD } from "../fixtures"
import { PHONE } from "./steerphone"
import { Glyph } from "./mobileui"

const CLAMP = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const
const EASED = { ...CLAMP, easing: EASE } as const

const PT = PHONE.screenW / 414
const pt = (v: number): number => Math.round(v * PT * 10) / 10

// The floating-bar chrome IssueScreen's bottom bar speaks.
const BAR_BG = "rgba(23,23,23,0.94)"
const BAR_SHADOW = "0 10px 30px rgba(0,0,0,0.5)"
const SECONDARY = "rgba(255,255,255,0.62)"

// The second file is brand new (status A) — a handful of added lines is all
// that ever reaches the screen under the first card.
const TEST_FILE_ROWS: DiffRow[] = [
  { t: "hunk", text: "@@ -0,0 +1,12 @@" },
  { t: "add", text: 'import { fireEvent, render } from "@testing-library/react"', new: 1 },
  { t: "add", text: 'import { PayButton } from "./PayButton"', new: 2 },
  { t: "add", text: "", new: 3 },
  { t: "add", text: 'test("ignores a second tap while paying", async () => {', new: 4 },
  { t: "add", text: "  const { getByRole } = render(<PayButton cart={cart} />)", new: 5 },
  { t: "add", text: '  fireEvent.click(getByRole("button"))', new: 6 },
]

const FILE_STATS = [
  { add: 12, del: CL_FILE_STATS.del },
  { add: CL_FILE_STATS.add - 12, del: 0 },
] as const

const FILE_ROWS: readonly DiffRow[][] = [CL_DIFF_ROWS, TEST_FILE_ROWS]

const pressT = (frame: number, at: number | undefined): number =>
  at === undefined ? 0 : interpolate(frame, [at, at + 3, at + 9], [0, 1, 0], CLAMP)

const Spinner: React.FC<{ frame: number; size: number }> = ({ frame, size }) => (
  <span style={{ display: "flex", rotate: `${(frame * 24) % 360}deg` }}>
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2.4}
      strokeLinecap="round"
    >
      <path d="M21 12a9 9 0 1 1-6.2-8.56" />
    </svg>
  </span>
)

// Middle-truncate a path the way the summary/file headers do on the phone.
const middle = (path: string, max: number): string => {
  if (path.length <= max) return path
  const keep = Math.floor((max - 1) / 2)
  return `${path.slice(0, keep)}…${path.slice(path.length - (max - 1 - keep))}`
}

// ── One file card: header row + the unified diff ─────────────────────────────
const FileCard: React.FC<{
  frame: number
  paintAt: number
  index: number
  file: { status: string; path: string }
  stats: { add: number; del: number }
  rows: readonly DiffRow[]
}> = ({ frame, paintAt, index, file, stats, rows }) => {
  const rise = interpolate(frame, [paintAt + index * 6, paintAt + index * 6 + 10], [0, 1], EASED)
  return (
    <div
      style={{
        boxSizing: "border-box",
        borderRadius: pt(16),
        backgroundColor: C.fillCard,
        border: `1px solid ${C.strokeRow}`,
        padding: pt(8),
        opacity: rise,
        translate: `0px ${(1 - rise) * pt(10)}px`,
      }}
    >
      {/* header: status letter · path · +/− · chevron */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: pt(8),
          padding: `${pt(4)}px ${pt(4)}px ${pt(10)}px`,
          fontFamily: MONO_FONT,
          fontSize: pt(13),
          color: C.text,
        }}
      >
        <span style={{ color: SECONDARY, fontWeight: 700 }}>{file.status}</span>
        <span
          style={{
            flex: 1,
            minWidth: 0,
            whiteSpace: "nowrap",
            overflow: "hidden",
            textOverflow: "ellipsis",
            fontWeight: 600,
          }}
        >
          {middle(file.path, 30)}
        </span>
        <span style={{ color: C.diffAdd }}>{`+${stats.add}`}</span>
        <span style={{ color: C.diffDel }}>{`−${stats.del}`}</span>
        <span style={{ color: SECONDARY, display: "flex" }}>
          <Glyph size={pt(13)} sw={2.2}>
            <path d="m18 15-6-6-6 6" />
          </Glyph>
        </span>
      </div>
      {/* the patch */}
      <div
        style={{
          borderRadius: pt(8),
          backgroundColor: "rgba(0,0,0,0.22)",
          padding: `${pt(6)}px 0`,
          overflow: "hidden",
        }}
      >
        {rows.map((row, i) => {
          const o = interpolate(
            frame,
            [paintAt + index * 6 + 4 + i * 1.1, paintAt + index * 6 + 10 + i * 1.1],
            [0, 1],
            CLAMP
          )
          const fg =
            row.t === "hunk"
              ? SECONDARY
              : row.t === "add"
                ? C.diffAdd
                : row.t === "del"
                  ? C.diffDel
                  : "#d4d4d4"
          const bg =
            row.t === "add" ? C.diffAddBgHot : row.t === "del" ? C.diffDelBgHot : undefined
          return (
            <div
              key={`${row.t}-${i}`}
              style={{
                display: "flex",
                height: pt(12.5),
                alignItems: "center",
                backgroundColor: bg,
                opacity: o,
                fontFamily: MONO_FONT,
                fontSize: pt(11.5),
                color: fg,
                whiteSpace: "pre",
                overflow: "hidden",
              }}
            >
              <span
                style={{
                  width: pt(18),
                  flexShrink: 0,
                  textAlign: "center",
                  color: row.t === "add" ? C.diffAdd : row.t === "del" ? C.diffDel : "transparent",
                }}
              >
                {row.t === "add" ? "+" : row.t === "del" ? "−" : " "}
              </span>
              <span>{row.text}</span>
            </div>
          )
        })}
      </div>
    </div>
  )
}

// ── The screen ───────────────────────────────────────────────────────────────
export type ReviewPhoneScreenProps = {
  frame: number
  /** The summary + file cards paint in from here. */
  paintAt: number
  /** Press flash on the Merge capsule (opens the confirm alert). */
  mergeTapAt: number
  /** The "Merge pull request?" alert is up from here … */
  confirmAt: number
  /** … until its Merge button is tapped here (capsule shows the spinner). */
  mergingAt: number
  /** The PR lands: pill → Merged, bar collapses to the GitHub circle. */
  mergedAt: number
}

export const ReviewPhoneScreen: React.FC<ReviewPhoneScreenProps> = ({
  frame,
  paintAt,
  mergeTapAt,
  confirmAt,
  mergingAt,
  mergedAt,
}) => {
  const merged = frame >= mergedAt
  const merging = frame >= mergingAt && !merged
  const capsulePress = pressT(frame, mergeTapAt)
  const alertPress = pressT(frame, mergingAt)

  // The alert: springs up at confirmAt, drops out right after the Merge tap.
  const alertIn =
    frame < confirmAt ? 0 : spring({ frame: frame - confirmAt, fps: 30, config: SETTLE })
  const alertOut = interpolate(frame, [mergingAt + 3, mergingAt + 8], [1, 0], EASED)
  const alertO = alertIn * alertOut

  // The bar after the merge: × and Merge collapse away, ↗ slides to center.
  const collapse = interpolate(frame, [mergedAt + 4, mergedAt + 14], [0, 1], EASED)
  // The leaving items are gone before their wrappers get narrow enough to
  // show a sliver of glyph.
  const leaveO = interpolate(collapse, [0, 0.4], [1, 0], CLAMP)
  const circle = pt(48)
  const capsuleW = pt(118)
  const gap = pt(12) * (1 - collapse)

  const summaryRise = interpolate(frame, [paintAt - 6, paintAt + 4], [0, 1], EASED)

  const barCircle = (children: React.ReactNode, style?: React.CSSProperties) => (
    <span
      style={{
        width: circle,
        height: circle,
        flexShrink: 0,
        borderRadius: 999,
        backgroundColor: BAR_BG,
        border: `1px solid ${C.strokeCard}`,
        boxShadow: BAR_SHADOW,
        color: SECONDARY,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        ...style,
      }}
    >
      {children}
    </span>
  )

  return (
    <div style={{ position: "absolute", inset: 0, fontFamily: UI_FONT }}>
      {/* nav bar: circular glass back button · "Review" */}
      <div
        style={{
          position: "absolute",
          top: pt(58),
          left: pt(18),
          right: pt(18),
          height: pt(42),
          display: "flex",
          alignItems: "center",
          color: C.muted,
          zIndex: 3,
        }}
      >
        <span
          style={{
            width: pt(42),
            height: pt(42),
            borderRadius: 999,
            backgroundColor: C.fillCard,
            border: `1px solid ${C.strokeRow}`,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            color: C.text,
          }}
        >
          <Glyph size={pt(18)} sw={2.4}>
            <path d="m15 18-6-6 6-6" />
          </Glyph>
        </span>
        <span
          style={{
            position: "absolute",
            left: 0,
            right: 0,
            textAlign: "center",
            fontSize: pt(17),
            fontWeight: 600,
            color: C.text,
          }}
        >
          Review
        </span>
      </div>

      {/* the scrolling review column */}
      <div
        style={{
          position: "absolute",
          top: pt(112),
          left: 0,
          right: 0,
          bottom: 0,
          overflow: "hidden",
        }}
      >
        <div
          style={{
            position: "absolute",
            left: pt(16),
            right: pt(16),
            top: 0,
            display: "flex",
            flexDirection: "column",
            gap: pt(10),
          }}
        >
          {/* summary card: branch · state pill · files +/− */}
          <div
            style={{
              boxSizing: "border-box",
              borderRadius: pt(16),
              backgroundColor: C.fillCard,
              border: `1px solid ${C.strokeRow}`,
              padding: pt(12),
              display: "flex",
              flexDirection: "column",
              gap: pt(8),
              opacity: summaryRise,
              translate: `0px ${(1 - summaryRise) * pt(8)}px`,
            }}
          >
            <span
              style={{
                fontFamily: MONO_FONT,
                fontSize: pt(13),
                color: SECONDARY,
                whiteSpace: "nowrap",
                overflow: "hidden",
                textOverflow: "ellipsis",
              }}
            >
              {CL_PR_HEAD.branch}
            </span>
            <span style={{ display: "flex", alignItems: "center", gap: pt(8) }}>
              <span
                style={{
                  height: pt(22),
                  boxSizing: "border-box",
                  display: "inline-flex",
                  alignItems: "center",
                  padding: `0 ${pt(8)}px`,
                  borderRadius: 999,
                  backgroundColor: merged ? C.tintDone : "rgba(255,255,255,0.10)",
                  border: `1px solid ${merged ? "rgba(59,130,246,0.35)" : C.strokeCard}`,
                  fontSize: pt(12),
                  fontWeight: 500,
                  color: C.text,
                }}
              >
                {merged ? "Merged" : "Open"}
              </span>
              <span style={{ fontSize: pt(13), color: SECONDARY }}>
                {`${CL_DIFF_FILES.length} files`}
              </span>
              <span style={{ fontFamily: MONO_FONT, fontSize: pt(13), color: C.diffAdd }}>
                {`+${CL_FILE_STATS.add}`}
              </span>
              <span style={{ fontFamily: MONO_FONT, fontSize: pt(13), color: C.diffDel }}>
                {`−${CL_FILE_STATS.del}`}
              </span>
            </span>
          </div>

          {CL_DIFF_FILES.map((file, i) => (
            <FileCard
              key={file.path}
              frame={frame}
              paintAt={paintAt}
              index={i}
              file={file}
              stats={FILE_STATS[i] ?? FILE_STATS[1]}
              rows={FILE_ROWS[i] ?? []}
            />
          ))}
        </div>
      </div>

      {/* floating bottom bar: × · Merge · ↗ (the merge buttons) */}
      <div
        style={{
          position: "absolute",
          left: 0,
          right: 0,
          bottom: pt(34),
          display: "flex",
          justifyContent: "center",
          alignItems: "center",
          zIndex: 4,
        }}
      >
        <span
          style={{
            width: circle * (1 - collapse),
            marginRight: gap,
            opacity: leaveO,
            overflow: "hidden",
            display: "flex",
            justifyContent: "center",
          }}
        >
          {barCircle(
            <Glyph size={pt(18)} sw={2.2}>
              <path d="M18 6 6 18" />
              <path d="m6 6 12 12" />
            </Glyph>
          )}
        </span>
        <span
          style={{
            width: capsuleW * (1 - collapse),
            marginRight: gap,
            opacity: leaveO,
            overflow: "hidden",
            display: "flex",
            justifyContent: "center",
          }}
        >
          <span
            style={{
              width: capsuleW,
              height: pt(52),
              flexShrink: 0,
              boxSizing: "border-box",
              borderRadius: 999,
              backgroundColor: BAR_BG,
              border: `1px solid ${C.strokeStrong}`,
              boxShadow: BAR_SHADOW,
              color: C.text,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              gap: pt(8),
              fontSize: pt(15),
              fontWeight: 500,
              scale: String(1 - 0.06 * capsulePress),
              filter: capsulePress > 0 ? `brightness(${1 + 0.25 * capsulePress})` : undefined,
            }}
          >
            {merging ? (
              <Spinner frame={frame} size={pt(17)} />
            ) : (
              <Glyph size={pt(17)} sw={2.2}>
                <circle cx="18" cy="18" r="3" />
                <circle cx="6" cy="6" r="3" />
                <path d="M6 21V9a9 9 0 0 0 9 9" />
              </Glyph>
            )}
            Merge
          </span>
        </span>
        {barCircle(
          <Glyph size={pt(17)} sw={2.2}>
            <path d="M15 3h6v6" />
            <path d="M10 14 21 3" />
            <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
          </Glyph>
        )}
      </div>

      {/* the "Merge pull request?" alert (the real iOS .alert) */}
      {alertO > 0.005 ? (
        <>
          <div
            style={{
              position: "absolute",
              inset: 0,
              backgroundColor: `rgba(0,0,0,${0.4 * alertO})`,
              zIndex: 8,
            }}
          />
          <div
            style={{
              position: "absolute",
              left: (PHONE.screenW - pt(270)) / 2,
              top: (PHONE.screenH - pt(150)) / 2,
              width: pt(270),
              boxSizing: "border-box",
              borderRadius: pt(14),
              backgroundColor: "rgba(36,36,38,0.98)",
              border: `1px solid rgba(255,255,255,0.10)`,
              boxShadow: "0 24px 60px rgba(0,0,0,0.6)",
              overflow: "hidden",
              opacity: alertO,
              scale: String(1.08 - 0.08 * alertIn),
              zIndex: 9,
              textAlign: "center",
            }}
          >
            <div style={{ padding: `${pt(18)}px ${pt(16)}px ${pt(16)}px` }}>
              <div style={{ fontSize: pt(17), fontWeight: 600, color: C.text }}>
                Merge pull request?
              </div>
              <div
                style={{
                  marginTop: pt(4),
                  fontSize: pt(13),
                  lineHeight: 1.35,
                  color: C.text,
                }}
              >
                {`Squash-merges PR #${CL.pr} via the GitHub App. Any live coding session for it closes.`}
              </div>
            </div>
            <div
              style={{
                display: "flex",
                borderTop: `1px solid rgba(255,255,255,0.12)`,
                height: pt(44),
                fontSize: pt(17),
              }}
            >
              <span
                style={{
                  flex: 1,
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  color: "#5fa8ff",
                  borderRight: `1px solid rgba(255,255,255,0.12)`,
                }}
              >
                Cancel
              </span>
              <span
                style={{
                  flex: 1,
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  color: "#5fa8ff",
                  fontWeight: 600,
                  backgroundColor: `rgba(255,255,255,${0.12 * alertPress})`,
                }}
              >
                Merge
              </span>
            </div>
          </div>
        </>
      ) : null}
    </div>
  )
}
