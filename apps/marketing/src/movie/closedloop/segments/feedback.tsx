// closedloop/segments/feedback.tsx — clip 4 (235f, calmed by EXP-388): a
// visitor hits the dead Pay-now button on acme.shop and reports it through
// the embedded feedback widget — the REAL flow (packages/widget, SLOP-4): the
// panel opens plain, Take screenshot lands in the full-screen annotator, a
// rect goes around the dead button, Done returns the thumbnail, the ONE
// "What happened?" message becomes the issue (title = first line). The
// camera eases out to the whole window while the annotator is up (its
// toolbar sits at the top of the page) and back in for the panel. After
// "Thanks, your report is in." the phone rises with the REAL mobile board
// list, the "New feedback: EXP-151" push drops and the issue row pops into
// Backlog — exactly the state clip 1 opens on, so the loop wraps into the
// live board. All beats are LOCAL frames.

import React from "react"
import { AbsoluteFill, interpolate } from "remotion"
import { PAGE_FONT } from "../../ships/theme"
import {
  Camera,
  Caption,
  CursorLayer,
  shotKeys,
  type CamKey,
  type CursorKey,
} from "../../ships/rig"
import {
  BrowserChassis,
  FeedbackFab,
  SITE_ANCHORS,
  SiteViewport,
} from "../surfaces/sitemock"
import { WIDGET_ANCHORS, WidgetPanel } from "../surfaces/widgetmock"
import { PhoneChassis } from "../surfaces/steerphone"
import { BoardScreen } from "../surfaces/mobileui"
import {
  CL,
  CL_PHONE_BOARD,
  COPY,
  NEW_ISSUE_ID,
  PUSH_FEEDBACK,
} from "../fixtures"
import { SEGMENT_DURATIONS } from "../timeline"
import {
  CLAMP_EASE,
  SegmentShell,
  captionSize,
  type SegmentProps,
} from "./common"

const DUR = SEGMENT_DURATIONS.feedback

// ── Beats (local frames) ──────────────────────────────────────────────────────
const B = {
  payClick: 26,
  fabHover: 48,
  fabClick: 58,
  panelAppear: 64,
  fabRest: 72,
  captureClick: 78, // Take screenshot → the annotator
  markFrom: 92, // rect drag around Pay now
  markTo: 102,
  doneClick: 112, // annotator Done → the panel with the thumbnail
  messageClick: 120,
  messageType: 124, // 70 chars at 3/frame → done by 148
  sendHover: 150,
  sendClick: 154,
  success: 162, // "Thanks, your report is in." holds
  phoneIn: 168, // the phone rises with the mobile board
  pushAt: 176, // "New feedback: EXP-151"
  insert: 190, // the EXP-151 row pops into Backlog
} as const

const CAPTIONS = {
  fb1: { in: 12, out: 60 },
  fb2: { in: 198, out: 226 },
} as const

// ── Camera ────────────────────────────────────────────────────────────────────
// ONE framing holds checkout + widget + the rising phone (EXP-388: no
// whip-pan, no camera moves).
const CAMERA_KEYS: CamKey[] = [
  { f: 0, s: 1.45, x: 1050, y: 600 },
  { f: B.captureClick, s: 1.45, x: 1050, y: 600 },
  // the annotator fills the page: ease out to the whole window
  { f: B.captureClick + 10, s: 1.0, x: 784, y: 500 },
  { f: B.doneClick, s: 1.0, x: 784, y: 500 },
  { f: B.doneClick + 10, s: 1.45, x: 1050, y: 600 },
]

// Portrait (EXP-482): three beats, three shots — the checkout card, the
// WHOLE widget panel, the WHOLE phone; every subject here is a tall card, so
// this is the most portrait-native clip. Never cut under a live caption:
// Caption holds to out+6, so fb1 (out 60) is only fully gone at 66 and the
// first cut waits for 67 rather than riding panelAppear at 64 — the panel
// gets a beat in shot A's corner first, which reads fine. The second cut is
// 2f before phoneIn and long clear of fb2 (in 198). Shot B runs caption-free
// by design: the annotator, the typed message and the success state are a
// show-don't-tell beat. Every shot stays inside the browser chassis — no
// wallpaper, unlike the wide framing.
const CAMERA_KEYS_PT: CamKey[] = shotKeys([
  { at: 0, s: 2.2, x: 1114, y: 430 }, // the checkout card + the dead Pay button
  { at: 67, s: 2.3, x: 1330, y: 663 }, // the WHOLE widget panel
  { at: 80, s: 1.2, x: 900, y: 480 }, // the annotator: toolbar + the marked button
  { at: 113, s: 2.3, x: 1330, y: 640 }, // the panel, now with its thumbnail
  { at: 166, s: 1.85, x: 930, y: 572 }, // the WHOLE phone: "New feedback"
])

// ── Cursor (site side) ────────────────────────────────────────────────────────
const PAY = SITE_ANCHORS.payButton
const FAB = SITE_ANCHORS.fab
const WC = WIDGET_ANCHORS.takeScreenshot
const MF = WIDGET_ANCHORS.markFrom
const MT = WIDGET_ANCHORS.markTo
const WDN = WIDGET_ANCHORS.annDone
const WM = WIDGET_ANCHORS.message
const WS = WIDGET_ANCHORS.send

const CURSOR_KEYS: CursorKey[] = [
  { f: 0, x: 900, y: 560 },
  { f: 20, x: PAY.x, y: PAY.y },
  { f: 40, x: PAY.x, y: PAY.y },
  { f: 48, x: FAB.x, y: FAB.y },
  { f: 64, x: FAB.x, y: FAB.y },
  { f: 74, x: WC.x, y: WC.y },
  { f: 80, x: WC.x, y: WC.y },
  { f: 90, x: MF.x, y: MF.y },
  { f: B.markFrom, x: MF.x, y: MF.y },
  { f: B.markTo, x: MT.x, y: MT.y },
  { f: 110, x: WDN.x, y: WDN.y },
  { f: 113, x: WDN.x, y: WDN.y },
  { f: 119, x: WM.x, y: WM.y },
  { f: 146, x: WM.x, y: WM.y },
  { f: 150, x: WS.x, y: WS.y },
  { f: 158, x: WS.x, y: WS.y },
  { f: 168, x: 1430, y: 900 },
]
const CURSOR_CLICKS = [
  B.payClick,
  B.fabClick,
  B.captureClick,
  B.markFrom,
  B.doneClick,
  B.messageClick,
  B.sendClick,
]

// Phone placement in COMP coordinates inside the camera layer (rises over
// the checkout page, left of the widget panel).
const PHONE_POS = { x: 950, y: 300, scale: 0.95 } as const

// ── The clip ──────────────────────────────────────────────────────────────────
export const FeedbackSegment: React.FC<SegmentProps> = ({
  frame,
  portrait,
}) => {
  const capSize = captionSize(portrait)

  const phoneRise = interpolate(
    frame,
    [B.phoneIn, B.phoneIn + 14],
    [0, 1],
    CLAMP_EASE
  )
  const insertT = interpolate(
    frame,
    [B.insert, B.insert + 14],
    [0, 1],
    CLAMP_EASE
  )

  return (
    <SegmentShell frame={frame} dur={DUR}>
      <AbsoluteFill>
        <Camera keys={portrait ? CAMERA_KEYS_PT : CAMERA_KEYS} frame={frame}>
          <BrowserChassis>
            <SiteViewport frame={frame} shakeAts={[B.payClick]} />
            <FeedbackFab
              frame={frame}
              hoverAt={B.fabHover}
              pressAt={B.fabClick}
              restAt={B.fabRest}
            />
            {frame >= B.panelAppear ? (
              <WidgetPanel
                frame={frame}
                appearAt={B.panelAppear}
                captureAt={B.captureClick}
                markFrom={B.markFrom}
                markTo={B.markTo}
                doneAt={B.doneClick}
                messageFocusAt={B.messageClick}
                messageTypeAt={B.messageType}
                sendHoverAt={B.sendHover}
                sendingAt={B.sendClick}
                successAt={B.success}
              />
            ) : null}
            <CursorLayer
              keys={CURSOR_KEYS}
              clicks={CURSOR_CLICKS}
              frame={frame}
              from={0}
              to={176}
            />
          </BrowserChassis>

          {/* the phone: the report lands as a push + a new Backlog row on
              the real mobile board */}
          {phoneRise > 0.01 ? (
            <div
              style={{
                position: "absolute",
                left: PHONE_POS.x,
                top: PHONE_POS.y,
                opacity: phoneRise,
                translate: `0px ${(1 - phoneRise) * 46}px`,
              }}
            >
              <div
                style={{
                  transform: `scale(${PHONE_POS.scale})`,
                  transformOrigin: "0 0",
                }}
              >
                <PhoneChassis glass={{ x: PHONE_POS.x, y: PHONE_POS.y }}>
                  <BoardScreen
                    frame={frame}
                    boardName={CL.board}
                    rows={CL_PHONE_BOARD}
                    insertId={NEW_ISSUE_ID}
                    moveT={insertT}
                    banner={{
                      at: B.pushAt,
                      title: PUSH_FEEDBACK.title,
                      body: PUSH_FEEDBACK.body,
                    }}
                  />
                </PhoneChassis>
              </div>
            </div>
          ) : null}
        </Camera>
      </AbsoluteFill>

      {(Object.keys(CAPTIONS) as (keyof typeof CAPTIONS)[]).map((key) => (
        <Caption
          key={key}
          frame={frame}
          in={CAPTIONS[key].in}
          out={CAPTIONS[key].out}
          size={capSize}
          centered={portrait}
          fontFamily={PAGE_FONT}
          letterSpacing="-0.03em"
        >
          {COPY[key]}
        </Caption>
      ))}
    </SegmentShell>
  )
}
