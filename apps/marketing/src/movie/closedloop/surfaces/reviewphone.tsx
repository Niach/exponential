// closedloop/surfaces/reviewphone.tsx — review + merge on the phone, as iOS
// does it today (shots/review-diff/ios.webp; ThreadScreen.guidePage →
// GuideFace's section page `GuideSectionDiffView` + WorkMergePill): the
// thread header (back · the thread's name · `…` once the run is idle), THE
// strip with the Guide tab shown (its green dot: the Guide holds the PR
// body), the back row "‹ Guide | Changes … +A −D", the DiffFileCards, and the
// floating bar cluster — the files circle (folder + count) beside the SOLID
// WHITE "Merge PR" capsule. Tap → THE app alert (GlassAlert: "Merge PR #n?" ·
// "It is squash-merged." · Cancel / Merge) → the capsule spins → merged: the
// host unmounts the capsule (it is mounted only while the PR is open) and the
// issue tab's glyph lands on Done (a merge moves its issue, issue-lifecycle).
//
// Every number is authored in iOS POINTS (414pt canvas) through `pt()`.
// All frame props are COMPOSITION-LOCAL to the segment that renders it.

import React from "react"
import { interpolate, spring } from "remotion"
import { EASE, SETTLE, UI_FONT } from "../../ships/theme"
import type { DiffRow } from "../../ships/fixtures"
import { CL, CL_DIFF_FILES, CL_DIFF_ROWS, CL_FILE_STATS, CL_ISSUE } from "../fixtures"
import { PHONE, ThreadChrome } from "./steerphone"
import {
  BarCircle,
  DIFF_IOS,
  DiffFileCard,
  Glyph,
  HAIRLINE,
  IOS,
  pt,
} from "./mobileui"

const CLAMP = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const
const EASED = { ...CLAMP, easing: EASE } as const

// The second file is brand new (status A).
const TEST_FILE_ROWS: DiffRow[] = [
  { t: "hunk", text: "@@ -0,0 +1,42 @@" },
  { t: "add", text: 'import { fireEvent, render } from "@testing-library/react"', new: 1 },
  { t: "add", text: 'import { PayButton } from "./PayButton"', new: 2 },
  { t: "add", text: "", new: 3 },
  { t: "add", text: 'test("ignores a second tap while paying", async () => {', new: 4 },
  { t: "add", text: "  const { getByRole } = render(<PayButton cart={cart} />)", new: 5 },
  { t: "add", text: '  fireEvent.click(getByRole("button"))', new: 6 },
]
const TEST_FILE = CL_DIFF_FILES[1]
const TOTAL = { add: CL_FILE_STATS.add + TEST_FILE.add, del: CL_FILE_STATS.del }

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

export type ReviewPhoneScreenProps = {
  frame: number
  /** The file cards paint in from here. */
  paintAt: number
  /** Press flash on the Merge PR capsule (opens the confirm alert). */
  mergeTapAt: number
  /** The alert is up from here … */
  confirmAt: number
  /** … until its Merge button is tapped here (the capsule spins). */
  mergingAt: number
  /** The PR lands: the capsule unmounts, the issue tab lands on Done. */
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

  const alertIn =
    frame < confirmAt ? 0 : spring({ frame: frame - confirmAt, fps: 30, config: SETTLE })
  const alertOut = interpolate(frame, [mergingAt + 3, mergingAt + 8], [1, 0], EASED)
  const alertO = alertIn * alertOut
  const capsuleOut = interpolate(frame, [mergedAt, mergedAt + 8], [1, 0], EASED)

  const rise = (i: number) =>
    interpolate(frame, [paintAt + i * 6, paintAt + i * 6 + 10], [0, 1], EASED)

  return (
    <div style={{ position: "absolute", inset: 0, fontFamily: UI_FONT }}>
      <ThreadChrome
        title={`#${CL_ISSUE.id} ${CL_ISSUE.title}`}
        running={false}
        issue={{ id: CL_ISSUE.id, title: CL_ISSUE.title, status: "in_review" }}
        issueStatusOverride={merged ? "done" : undefined}
        shown="guide"
        guideDot
      />

      {/* the section page's back row: ‹ Guide | Changes … +A −D */}
      <div
        style={{
          position: "absolute",
          top: pt(154),
          left: pt(16),
          right: pt(16),
          height: pt(36),
          display: "flex",
          alignItems: "center",
          gap: pt(8),
        }}
      >
        <span style={{ display: "flex", alignItems: "center", gap: pt(4), color: "#ffffff" }}>
          <Glyph size={pt(14)} sw={2.2}>
            <path d="m15 18-6-6 6-6" />
          </Glyph>
          <span style={{ fontSize: pt(15), fontWeight: 500 }}>Guide</span>
        </span>
        <span style={{ width: HAIRLINE, height: pt(14), backgroundColor: IOS.strokeCard }} />
        <span style={{ fontSize: pt(15), color: IOS.secondary }}>Changes</span>
        <span style={{ flex: 1 }} />
        <span style={{ fontSize: pt(15), color: DIFF_IOS.addFg }}>{`+${TOTAL.add}`}</span>
        <span style={{ fontSize: pt(15), color: DIFF_IOS.delFg }}>{`−${TOTAL.del}`}</span>
      </div>

      {/* the diff cards (DiffFileList: one glass card per file, 12pt apart) */}
      <div
        style={{
          position: "absolute",
          top: pt(196),
          left: pt(16),
          right: pt(16),
          bottom: 0,
          display: "flex",
          flexDirection: "column",
          gap: pt(12),
          overflow: "hidden",
        }}
      >
        <div style={{ opacity: rise(0), translate: `0px ${(1 - rise(0)) * pt(10)}px` }}>
          <DiffFileCard
            status={CL_DIFF_FILES[0].status}
            path={CL_DIFF_FILES[0].path}
            add={CL_FILE_STATS.add}
            del={CL_FILE_STATS.del}
            rows={CL_DIFF_ROWS}
            unchanged={20}
            maxPathChars={32}
            rowOpacity={(i) =>
              interpolate(frame, [paintAt + 4 + i * 1.1, paintAt + 10 + i * 1.1], [0, 1], CLAMP)
            }
          />
        </div>
        <div style={{ opacity: rise(1), translate: `0px ${(1 - rise(1)) * pt(10)}px` }}>
          <DiffFileCard
            status={TEST_FILE.status}
            path={TEST_FILE.path}
            add={TEST_FILE.add}
            del={TEST_FILE.del}
            rows={TEST_FILE_ROWS}
            maxPathChars={32}
          />
        </div>
      </div>

      {/* the floating bar cluster: files circle · the Merge PR capsule */}
      <div
        style={{
          position: "absolute",
          left: 0,
          right: 0,
          bottom: pt(4) + pt(34),
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          gap: pt(12),
          zIndex: 3,
        }}
      >
        <BarCircle style={{ flexDirection: "column", gap: pt(2) }}>
          <Glyph size={pt(18)} sw={2}>
            <path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z" />
          </Glyph>
          <span style={{ fontSize: pt(11), color: IOS.secondary, lineHeight: 1 }}>
            {CL_DIFF_FILES.length}
          </span>
        </BarCircle>
        {capsuleOut > 0.01 ? (
          <span
            style={{
              height: pt(52),
              boxSizing: "border-box",
              padding: `0 ${pt(24)}px`,
              borderRadius: 999,
              backgroundColor: "#ffffff",
              color: "rgba(0,0,0,0.9)",
              display: "flex",
              alignItems: "center",
              gap: pt(8),
              fontSize: pt(15),
              fontWeight: 500,
              opacity: capsuleOut,
              scale: String((1 - 0.04 * capsulePress) * (0.9 + 0.1 * capsuleOut)),
            }}
          >
            {merging ? (
              <span style={{ color: "rgba(0,0,0,0.6)", display: "flex" }}>
                <Spinner frame={frame} size={pt(16)} />
              </span>
            ) : (
              <Glyph size={pt(20)} sw={2}>
                <circle cx="18" cy="18" r="3" />
                <circle cx="6" cy="6" r="3" />
                <path d="M6 21V9a9 9 0 0 0 9 9" />
              </Glyph>
            )}
            Merge PR
          </span>
        ) : null}
      </div>

      {/* THE app alert (GlassAlert, Prompts.MergeIssuePr) */}
      {alertO > 0.005 ? (
        <>
          <div
            style={{
              position: "absolute",
              inset: 0,
              backgroundColor: `rgba(0,0,0,${0.6 * alertO})`,
              zIndex: 8,
            }}
          />
          <div
            style={{
              position: "absolute",
              left: pt(24),
              right: pt(24),
              top: PHONE.screenH / 2 - pt(75),
              boxSizing: "border-box",
              padding: pt(20),
              borderRadius: pt(16),
              backgroundColor: IOS.backgroundBottom,
              boxShadow: `inset 0 0 0 ${HAIRLINE}px ${IOS.strokeCard}, 0 24px 60px rgba(0,0,0,0.6)`,
              opacity: alertO,
              scale: String(1.06 - 0.06 * alertIn),
              zIndex: 9,
            }}
          >
            <div style={{ fontSize: pt(18), fontWeight: 600, color: "#fafafa" }}>
              {`Merge PR #${CL.pr}?`}
            </div>
            <div style={{ marginTop: pt(8), fontSize: pt(15), color: IOS.muted }}>
              It is squash-merged.
            </div>
            <div
              style={{
                marginTop: pt(16),
                display: "flex",
                justifyContent: "flex-end",
                gap: pt(8),
              }}
            >
              <span
                style={{
                  height: pt(32),
                  boxSizing: "border-box",
                  padding: `0 ${pt(12)}px`,
                  borderRadius: 999,
                  boxShadow: `inset 0 0 0 ${HAIRLINE}px ${IOS.strokeStrong}`,
                  display: "flex",
                  alignItems: "center",
                  fontSize: pt(15),
                  fontWeight: 500,
                  color: "#ffffff",
                }}
              >
                Cancel
              </span>
              <span
                style={{
                  height: pt(32),
                  boxSizing: "border-box",
                  padding: `0 ${pt(12)}px`,
                  borderRadius: 999,
                  backgroundColor: IOS.primary,
                  display: "flex",
                  alignItems: "center",
                  fontSize: pt(15),
                  fontWeight: 500,
                  color: IOS.primaryFg,
                  filter: alertPress > 0 ? `brightness(${1 - 0.15 * alertPress})` : undefined,
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
