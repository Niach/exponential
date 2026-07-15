// spot/Spot.tsx — the 36s LaunchSpot film (f0–990; the outro sequence follows).
// Live-action Seedance layers sit ABOVE the UI/camera layer (they fade out to
// reveal it); big type on cards, live bookends, and two in-beat captions.
// Two shells share the chassis: the hero story (U1 issue pick + U2 dock run,
// one continuous session) and the release waves (U3, entered on a hard cut).
// The only cursor in the spot is U1's pick-and-start choreography.

import React from "react"
import { AbsoluteFill, interpolate, spring, useCurrentFrame } from "remotion"
import { C, SETTLE, WIN } from "../ships/theme"
import { BOARD, HERO, HERO_SESSION, type BoardRow, type IssueStatus } from "../ships/fixtures"
import { Camera, CursorLayer, Punch, WindowChassis, WordmarkChip } from "../ships/rig"
import { BoardActions, BoardTool, SidebarPane } from "../ships/surfaces/board"
import { CenterEmptyState, DockCollapsedStrip, IconRail, TabsBar, TopBar, type ChromeTab } from "../ships/surfaces/chrome"
import { IssueDetailPane } from "../ships/surfaces/detail"
import { FlowGraph } from "../ships/surfaces/flowgraph"
import { PhonePiP } from "../ships/surfaces/phone"
import { ReleaseDetailTool, ReleasesTool } from "../ships/surfaces/releases"
import { TerminalDock, type DockTab } from "../ships/surfaces/terminal"
import { FOOTAGE, FootageClip } from "./footage"
import { TypeCard } from "./overlays"
import {
  CASCADE_DONE_AT,
  CODING_NOW_AT,
  CURSOR_U1,
  CURSOR_U1_KEYS,
  DETAIL_IN_AT,
  DOCK_INPUT_GLOW_AT,
  FLOW_SCHEDULE,
  HERO_DOCK_OPEN_AT,
  HERO_FEED_SCHEDULE,
  ORCH_EVENTS,
  ORCH_SCHEDULE,
  OVERLAYS,
  PHONE_FEED_SCHEDULE,
  PHONE_IN_AT,
  PHONE_OUT,
  PHONE_POS,
  PHONE_PULSE_AT,
  PHONE_SCALE,
  PROGRESS_STEPS,
  PR_CHIP_AT,
  RELEASE_DRILL_AT,
  RELEASE_PR_CHIP,
  ROW_CLICK_AT,
  ROW_HOVER,
  SEG,
  SESSION_EXIT_AT,
  SHELL_SWITCH_AT,
  SHIPPED_AT,
  SPOT_CAMERA_KEYS,
  SPOT_COPY,
  START_HOVER,
  STATUS_FLIPS,
  STATUS_FLIP_AT,
  UI_UNMOUNT_AT,
} from "./timeline"

const CLAMP = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const

// Center-pane geometry (window-local)
const CENTER_X = WIN.rail + WIN.sidebar
const CENTER_W = WIN.w - CENTER_X
const CONTENT_TOP = WIN.topBar + WIN.dockTabs

const TAB_EXP: ChromeTab = { id: "exp142", label: HERO.id, mono: true }
const TAB_REL: ChromeTab = { id: "rel", label: "Release v0.12" }
const HERO_TAB_POP: Record<string, number> = { exp142: DETAIL_IN_AT }

const HERO_DOCK_TABS: DockTab[] = [
  { id: "zsh", label: "zsh" },
  { id: "hero", label: HERO.sessionTab, dot: C.green },
]
const ORCH_DOCK_TABS: DockTab[] = [
  { id: "orch", label: "Release v0.12", dot: C.green, popAt: 562 },
  { id: "zsh", label: "zsh" },
  { id: "w139", label: "claude · EXP-139", dot: C.green, popAt: 612 },
  { id: "w141", label: "claude · EXP-141", dot: C.green, popAt: 615 },
  { id: "w143", label: "claude · EXP-143", dot: C.green, popAt: 618 },
  { id: "w144", label: "claude · EXP-144", dot: C.green, popAt: 652 },
  { id: "w145", label: "claude · EXP-145", dot: C.green, popAt: 655 },
]

// ── U1 + U2 — the hero shell: board pick → detail → Start coding → dock run ──
const HeroShell: React.FC<{ frame: number }> = ({ frame }) => {
  const dockH = frame < HERO_DOCK_OPEN_AT ? WIN.dockStrip : WIN.dockExpanded
  const paneH = WIN.h - CONTENT_TOP - dockH
  const heroStatus: IssueStatus = frame >= STATUS_FLIP_AT ? "in_progress" : "todo"
  const overrides: Record<string, Partial<BoardRow>> = { [HERO.id]: { status: heroStatus } }
  return (
    <>
      <TopBar frame={frame} />
      <IconRail frame={frame} active="issues" />
      <TabsBar
        frame={frame}
        tabs={frame < DETAIL_IN_AT ? [] : [TAB_EXP]}
        activeId="exp142"
        popAt={HERO_TAB_POP}
      />
      <SidebarPane title="All Issues" actions={<BoardActions />} pills bottomInset={dockH}>
        <BoardTool
          frame={frame}
          rows={BOARD}
          overrides={overrides}
          hover={{ id: HERO.id, from: ROW_HOVER.from, to: ROW_HOVER.to }}
          selectedId={frame >= ROW_CLICK_AT ? HERO.id : undefined}
          prDotId={{ id: HERO.id, at: PR_CHIP_AT }}
          showLabels={false}
        />
      </SidebarPane>
      {frame < DETAIL_IN_AT + 8 ? (
        <div style={{ opacity: interpolate(frame, [DETAIL_IN_AT, DETAIL_IN_AT + 6], [1, 0], CLAMP) }}>
          <CenterEmptyState frame={frame} bottom={dockH} />
        </div>
      ) : null}
      {frame >= DETAIL_IN_AT ? (
        <div
          style={{
            position: "absolute",
            left: CENTER_X,
            top: CONTENT_TOP,
            width: CENTER_W,
            height: paneH,
            overflow: "hidden",
          }}
        >
          <IssueDetailPane
            frame={frame}
            status={heroStatus}
            slideInAt={DETAIL_IN_AT}
            staggerAt={DETAIL_IN_AT + 6}
            startHover={{ at: START_HOVER.at, out: START_HOVER.out }}
            codingNow={{ at: CODING_NOW_AT, out: SESSION_EXIT_AT }}
            prChip={{ at: PR_CHIP_AT }}
            width={CENTER_W}
            height={paneH}
          />
        </div>
      ) : null}
      {frame < HERO_DOCK_OPEN_AT ? (
        <DockCollapsedStrip frame={frame} count={1} />
      ) : (
        <TerminalDock
          frame={frame}
          height={WIN.dockExpanded}
          tabs={HERO_DOCK_TABS}
          activeTab="hero"
          feed={{ events: HERO_SESSION, schedule: HERO_FEED_SCHEDULE }}
          inputGlow={DOCK_INPUT_GLOW_AT}
          exitAt={SESSION_EXIT_AT}
          exitBadgeTab="hero"
        />
      )}
      {/* the spot's only cursor: pick the issue, start the coding */}
      <CursorLayer
        keys={CURSOR_U1_KEYS}
        clicks={[...CURSOR_U1.clicks]}
        frame={frame}
        from={CURSOR_U1.from}
        to={CURSOR_U1.to}
      />
    </>
  )
}

// ── U3 — the release-waves shell (hard cut in; list → drill → waves) ──────────
const ReleaseShell: React.FC<{ frame: number }> = ({ frame }) => (
  <>
    <TopBar frame={frame} />
    <IconRail frame={frame} active="releases" />
    <TabsBar frame={frame} tabs={[TAB_EXP, TAB_REL]} activeId="rel" />
    <div
      style={{
        position: "absolute",
        left: WIN.rail,
        top: WIN.topBar,
        width: WIN.sidebar,
        height: WIN.h - WIN.topBar - WIN.dockExpanded,
        backgroundColor: C.panel,
        borderRight: `1px solid ${C.border}`,
        overflow: "hidden",
      }}
    >
      <ReleasesTool frame={frame} exitAt={RELEASE_DRILL_AT} />
      <ReleaseDetailTool
        frame={frame}
        drillAt={RELEASE_DRILL_AT}
        progress={PROGRESS_STEPS}
        statusFlipAt={STATUS_FLIPS}
        shippedAt={SHIPPED_AT}
        prChip={RELEASE_PR_CHIP}
        cascadeDoneAt={CASCADE_DONE_AT}
      />
    </div>
    <div
      style={{
        position: "absolute",
        left: CENTER_X,
        top: CONTENT_TOP,
        width: CENTER_W,
        height: WIN.h - CONTENT_TOP - WIN.dockExpanded,
        overflow: "hidden",
      }}
    >
      <FlowGraph
        frame={frame}
        schedule={FLOW_SCHEDULE}
        width={CENTER_W}
        height={WIN.h - CONTENT_TOP - WIN.dockExpanded}
        padTop={214}
      />
    </div>
    <TerminalDock
      frame={frame}
      height={WIN.dockExpanded}
      tabs={ORCH_DOCK_TABS}
      activeTab="orch"
      feed={{ events: ORCH_EVENTS, schedule: ORCH_SCHEDULE }}
      spinnerTip={null}
    />
  </>
)

// ── Phone PiP (screen-space, 1.24× the film's size) ───────────────────────────
const SpotPhone: React.FC<{ frame: number }> = ({ frame }) => {
  if (frame < PHONE_IN_AT - 4 || frame > PHONE_OUT.to) return null
  const slideIn = frame < PHONE_IN_AT ? 0 : spring({ frame: frame - PHONE_IN_AT, fps: 30, config: SETTLE })
  const exitT = interpolate(frame, [PHONE_OUT.from, PHONE_OUT.to], [0, 1], CLAMP)
  return (
    <AbsoluteFill style={{ transformOrigin: `${PHONE_POS.x}px ${PHONE_POS.y}px`, scale: String(PHONE_SCALE) }}>
      <PhonePiP
        frame={frame}
        x={PHONE_POS.x + 460 * (1 - slideIn) + 60 * exitT}
        y={PHONE_POS.y}
        rotate={2 - 2 * slideIn}
        feedSchedule={PHONE_FEED_SCHEDULE}
        sendPulseAt={PHONE_PULSE_AT}
        opacity={1 - exitT}
      />
    </AbsoluteFill>
  )
}

// ── The spot ──────────────────────────────────────────────────────────────────
export const Spot: React.FC = () => {
  const frame = useCurrentFrame() // global (Sequence from=0)

  return (
    <AbsoluteFill>
      {/* UI world — unmounts under card3's opaque plateau (never flash UI on its lift) */}
      {frame >= SEG.uiFadeIn && frame < UI_UNMOUNT_AT ? (
        <Camera keys={SPOT_CAMERA_KEYS} frame={frame}>
          <WindowChassis>
            {frame < SHELL_SWITCH_AT ? <HeroShell frame={frame} /> : <ReleaseShell frame={frame} />}
          </WindowChassis>
        </Camera>
      ) : null}
      <SpotPhone frame={frame} />

      {/* live-action layers (above the UI; B fades out to reveal the board) */}
      {/* the take's café ambience is quiet (−26 LUFS, peak −7.2dB) — ×2 makes it the LOUD state */}
      <FootageClip spec={FOOTAGE.chaos} from={SEG.liveA} duration={SEG.liveB - SEG.liveA} volume={() => 2} />
      <FootageClip
        spec={FOOTAGE.typing}
        from={SEG.liveB}
        duration={SEG.uiFadeIn + 9 - SEG.liveB}
        fadeOut={18}
        // the §5 duck: continue the boosted chaos level, then we own the fall to
        // keystrokes-only (local f30–75 = global f150–195), whatever the take's mix does
        volume={(f) => interpolate(f, [0, 30, 75], [2, 1.4, 0.45], CLAMP)}
      />
      <FootageClip
        spec={FOOTAGE.payoff}
        from={SEG.liveC}
        duration={SEG.filmEnd - SEG.liveC}
        fadeOut={8}
        // the take's actor starts talking at source ~6.5s (measured spike) — the
        // very end of the shown window (0.2–6.6s). Silent from local f174 (source 6.0s).
        volume={(f) => interpolate(f, [168, 174], [1, 0], CLAMP)}
      />

      {/* type — cards, live bookends, and the two in-beat captions. The chip
          renders FIRST so the opaque cards cover it (card1 carries its own
          brand lockup — no double branding). */}
      <WordmarkChip frame={frame} in={OVERLAYS.wordmark.in} out={OVERLAYS.wordmark.out} />
      <Punch frame={frame} in={OVERLAYS.liveA.in} out={OVERLAYS.liveA.out} lines={[SPOT_COPY.liveA]} />
      <TypeCard frame={frame} in={OVERLAYS.card1.in} out={OVERLAYS.card1.out} brand>
        <div>{SPOT_COPY.hook1}</div>
        <div>{SPOT_COPY.hook2}</div>
      </TypeCard>
      <Punch frame={frame} in={OVERLAYS.steer.in} out={OVERLAYS.steer.out} lines={[SPOT_COPY.steer]} size={56} />
      <Punch frame={frame} in={OVERLAYS.release.in} out={OVERLAYS.release.out} lines={[SPOT_COPY.release]} size={56} />
      <TypeCard frame={frame} in={OVERLAYS.card3.in} out={OVERLAYS.card3.out}>
        {SPOT_COPY.card3}
      </TypeCard>
      <Punch frame={frame} in={OVERLAYS.liveC.in} out={OVERLAYS.liveC.out} lines={[SPOT_COPY.liveC]} size={72} weight={700} y={880} />
    </AbsoluteFill>
  )
}
