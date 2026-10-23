// closedloop/segments/codeeverywhere.tsx — clip 2 (260f, EXP-385: the merged
// start-coding + live-steer clip). Code from everywhere, running locally:
// the phone rises with EXP-151's REAL mobile issue view, Start pushes the
// Agent page (the issue chip, the optional instructions, the device /
// account / model / Plan pills) and on send the desktop reacts
// SIMULTANEOUSLY — the session takes the issue (issue-lifecycle `taken`:
// Backlog → In Progress), the issue's "Threads" row and the sidebar's
// Active section gain the new thread. A click on the thread chip opens the
// thread (header · Stop · the strip · the transcript · the composer) and the
// transcript streams Trace lines, prose and the edited-files card. The phone
// opens the same thread; a typed steer lands in the desktop transcript as
// your message and the agent picks it up. All beats are LOCAL frames.

import React from "react"
import { AbsoluteFill, interpolate } from "remotion"
import { PAGE_FONT } from "../../ships/theme"
import {
  Camera,
  Caption,
  WindowChassis,
  shotKeys,
  type CamKey,
} from "../../ships/rig"
import { CursorLayer, type CursorKey } from "../../ships/rig"
import {
  CutoutPanel,
  PANEL,
  Sidebar,
  TitleBar,
  panelPt,
  type ShellTab,
} from "../../ships/surfaces/shell"
import { IssueView } from "../../ships/surfaces/issueview"
import {
  Composer,
  ThreadHeader,
  Transcript,
  type TranscriptItem,
} from "../../ships/surfaces/threadview"
import { AnnotatedScreenshot, SCREENSHOT_ASPECT } from "../surfaces/screenshot"
import { StartPhone } from "../surfaces/startphone"
import { SteerPhone } from "../surfaces/steerphone"
import {
  CL,
  CL_BOARDS,
  CL_DESKTOP_ISSUE,
  CL_ISSUE,
  CL_STEER_MSG,
  CL_THREAD_TITLE,
  COPY,
  NEW_ISSUE_ID,
} from "../fixtures"
import { SEGMENT_DURATIONS } from "../timeline"
import {
  CLAMP,
  CLAMP_EASE,
  SegmentShell,
  captionSize,
  type SegmentProps,
} from "./common"

const DUR = SEGMENT_DURATIONS["code-everywhere"]

// ── Beats (local frames) ──────────────────────────────────────────────────────
const B = {
  phoneIn: 8, // the iPhone rises with the EXP-151 issue view
  tapAt: 28, // Start-circle tap on the issue view
  sheetAt: 34, // the Agent page pushes in — then DWELLS (~70f)
  flick: { at: 58, out: 66 }, // hover flick across the Agent pill
  startAt: 104, // the send press → "Starting on …"
  simul: 112, // the desktop reacts: In Progress, Threads row, Active row
  sheetOut: 112, // the phone opens the new thread
  chipClick: 124, // the desktop click on the issue's thread chip
  sessionSlide: 128, // the thread opens in the panel
  feed: [134, 142, 150, 158, 168] as const, // the first transcript items
  phoneSwap: 136, // the phone flips to the thread screen
  phoneFeed: [142, 152, 162] as const, // the phone's mirror rows
  typeAt: 172, // steer typing on the phone (2 cpf)
  sendAt: 210, // send tap → user bubble
  steerGlow: 214, // the composer pulses as the steer arrives
  steerLand: 218, // the steer lands in the transcript as your message
  reply: [226, 238, 250] as const, // the reply
} as const

const CAPTIONS = {
  ce1: { in: 20, out: 100 },
  ce2: { in: 116, out: 162 },
  ce3: { in: 214, out: 250 },
} as const

// The transcript (EXP-1321 Trace sm lines, EXP-916 one edited-files card
// for the consecutive edits), your steer, then the reply.
const PB = "apps/shop/src/checkout/PayButton.tsx"
const TEST = "apps/shop/src/checkout/pay-button.test.tsx"
const FEED_ITEMS: TranscriptItem[] = [
  { kind: "trace", icon: "read", verb: "Read", detail: PB },
  {
    kind: "prose",
    text: "The submit handler bails while cart revalidation is pending. On Safari that promise never settles, so every click is swallowed. Fixing the pending state and re-enabling submit:",
  },
  {
    kind: "edits",
    files: [
      { path: PB, add: 18, del: 6 },
      { path: TEST, add: 42, del: 0, at: 158 },
    ],
  },
  { kind: "trace", icon: "terminal", verb: "Ran", detail: "bun run typecheck" },
  {
    kind: "user",
    text: CL_STEER_MSG,
    byline: `${CL.user} · 9:41`,
  },
  {
    kind: "prose",
    text: "Good catch. Adding an in-flight guard and a double-submit test.",
  },
  { kind: "edits", files: [{ path: PB, add: 9, del: 0 }] },
  { kind: "trace", icon: "terminal", verb: "Ran", detail: "bun test checkout" },
]
const FEED_SCHEDULE = [B.feed[0], B.feed[1], B.feed[2], B.feed[4], B.steerLand, ...B.reply]

// ── Camera ────────────────────────────────────────────────────────────────────
// ONE framing holds the WHOLE window — full issue detail + properties
// sidebar (where the coding-now pill pops) and the dock — with the phone
// floating over the window's LEFT edge so it never covers that sidebar.
// The steer lands while the framing stands still (EXP-388: no camera moves).
const CAMERA_KEYS: CamKey[] = [{ f: 0, s: 1.06, x: 790, y: 513 }]

// Portrait (EXP-482): the phone IS the story — and at 1080×1350 the whole
// device fits the frame (comp rect ~380×780 at its 1.15 scale), composer
// and all, instead of the old sheet-only crop. Two shots: the cut sits 4f
// before `simul`, so the dock springs open ON camera in shot B, and in the
// caption gap (ce1 is gone by 106, ce2 arrives at 116). Shot B slides right
// and down so a band of the bottom terminal tab bar rides beside the phone; the
// steer must be SEEN landing there.
const CAMERA_KEYS_PT: CamKey[] = shotKeys([
  { at: 0, s: 1.7, x: 224, y: 610 }, // the WHOLE phone, Agent page up
  { at: 108, s: 1.7, x: 334, y: 590 }, // phone session feed + dock band
])

// The window's ONE tab follows the navigation: the issue, then the thread
// (its lead = the root's state mark, session-card `working` = play).
const TAB = (frame: number): ShellTab =>
  frame >= B.sessionSlide
    ? { id: "tab", lead: { kind: "running" }, title: CL_THREAD_TITLE }
    : {
        id: "tab",
        lead: {
          kind: "status",
          status: frame >= B.simul ? "in_progress" : "backlog",
        },
        identifier: NEW_ISSUE_ID,
        title: CL_ISSUE.title,
      }

// The description's screenshot: 420 tall (image_root_max_height).
const SHOT_IMAGE = {
  node: <AnnotatedScreenshot width={420 * SCREENSHOT_ASPECT} />,
  w: 420 * SCREENSHOT_ASPECT,
  h: 420,
}

// The desktop pointer: the thread chip in the issue's Threads row (the
// sidebar is behind the phone in this framing).
const CHIP = panelPt(290, 54)
const CURSOR_KEYS: CursorKey[] = [
  { f: 96, x: 1000, y: 330 },
  { f: 120, x: CHIP.x, y: CHIP.y },
  { f: 140, x: CHIP.x + 40, y: CHIP.y + 120 },
]

// Phone placement in COMP coordinates inside the camera layer — over the
// window's LEFT edge (the board is carried context; the detail pane and its
// properties sidebar stay clear on the right).
const PHONE_POS = { x: 210, y: 268, scale: 1.15 } as const

// ── The clip ──────────────────────────────────────────────────────────────────
export const CodeEverywhereSegment: React.FC<SegmentProps> = ({
  frame,
  portrait,
}) => {
  const slide = interpolate(
    frame,
    [B.sessionSlide, B.sessionSlide + 10],
    [0, 1],
    CLAMP_EASE
  )
  const capSize = captionSize(portrait)
  const heroStatus =
    frame >= B.simul ? ("in_progress" as const) : ("backlog" as const)
  const panelW = PANEL.w - 2
  const panelH = PANEL.h - 2

  const phoneRise = interpolate(
    frame,
    [B.phoneIn, B.phoneIn + 14],
    [0, 1],
    CLAMP_EASE
  )
  // Start view → steer view crossfade while the sheet collapses.
  const startO = interpolate(
    frame,
    [B.phoneSwap, B.phoneSwap + 8],
    [1, 0],
    CLAMP
  )

  return (
    <SegmentShell frame={frame} dur={DUR}>
      <AbsoluteFill>
        <Camera keys={portrait ? CAMERA_KEYS_PT : CAMERA_KEYS} frame={frame}>
          <WindowChassis>
            <TitleBar frame={frame} tabs={[TAB(frame)]} activeId="tab" />
            <Sidebar
              frame={frame}
              team={CL.team}
              user={CL.user.split(" ")[0]}
              initials={CL.initials}
              selected={frame >= B.sessionSlide ? "thread:exp151" : undefined}
              inboxDotAt={0}
              boards={[...CL_BOARDS]}
              active={[{ id: "exp151", title: CL_THREAD_TITLE, at: B.simul }]}
            />
            <CutoutPanel>
              {slide < 1 ? (
                <div style={{ position: "absolute", inset: 0, opacity: 1 - slide }}>
                  <IssueView
                    frame={frame}
                    issue={CL_DESKTOP_ISSUE}
                    status={heroStatus}
                    statusAt={B.simul}
                    thread={{ title: CL_THREAD_TITLE, at: B.simul }}
                    image={SHOT_IMAGE}
                    width={panelW}
                    height={panelH}
                  />
                </div>
              ) : null}
              {slide > 0 ? (
                <div style={{ position: "absolute", inset: 0, opacity: slide }}>
                  <ThreadHeader
                    frame={frame}
                    title={CL_THREAD_TITLE}
                    issue={{ id: NEW_ISSUE_ID, title: CL_ISSUE.title, status: heroStatus }}
                    stop
                    face="conversation"
                    width={panelW}
                  />
                  <Transcript
                    frame={frame}
                    items={FEED_ITEMS}
                    schedule={FEED_SCHEDULE}
                    width={panelW}
                    height={panelH}
                  />
                  <Composer
                    frame={frame}
                    width={panelW}
                    height={panelH}
                    model="Opus"
                    glowAt={B.steerGlow}
                    busy
                  />
                </div>
              ) : null}
            </CutoutPanel>
            <CursorLayer
              keys={CURSOR_KEYS}
              clicks={[B.chipClick]}
              frame={frame}
              from={96}
              to={140}
            />
          </WindowChassis>

          {/* the phone, floating over the window's left edge (comp coords).
              It composes AFTER the chassis, which isolates its own stacking
              (EXP-823), so the whole window stays behind it. */}
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
              {startO > 0 ? (
                <div style={{ opacity: startO }}>
                  <StartPhone
                    frame={frame}
                    tapAt={B.tapAt}
                    sheetAt={B.sheetAt}
                    flickAt={B.flick}
                    startAt={B.startAt}
                    collapseAt={B.sheetOut}
                    glass={{ x: PHONE_POS.x, y: PHONE_POS.y }}
                  />
                </div>
              ) : null}
              {frame >= B.phoneSwap ? (
                <div
                  style={{
                    position: "absolute",
                    left: 0,
                    top: 0,
                    opacity: 1 - startO,
                  }}
                >
                  <SteerPhone
                    frame={frame}
                    feedSchedule={B.phoneFeed}
                    typeAt={B.typeAt}
                    sendAt={B.sendAt}
                    glass={{ x: PHONE_POS.x, y: PHONE_POS.y }}
                  />
                </div>
              ) : null}
            </div>
          </div>
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
