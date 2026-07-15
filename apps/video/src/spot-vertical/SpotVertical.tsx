// spot-vertical/SpotVertical.tsx — the 15s 9:16 film body (f0–378; the outro
// sequence follows in the composition root). One hero beat: board pick →
// detail → Start coding → the status flips — then the "Shipped." card says
// the rest. Portrait camera; shares the ships/ surfaces, owns everything else.

import React from "react"
import { AbsoluteFill, interpolate, useCurrentFrame } from "remotion"
import { WIN } from "../ships/theme"
import { BOARD, HERO, type BoardRow, type IssueStatus } from "../ships/fixtures"
import { camAt, CursorLayer, WindowChassis, type CamKey } from "../ships/rig"
import { BoardActions, BoardTool, SidebarPane } from "../ships/surfaces/board"
import { CenterEmptyState, DockCollapsedStrip, IconRail, TabsBar, TopBar, type ChromeTab } from "../ships/surfaces/chrome"
import { IssueDetailPane } from "../ships/surfaces/detail"
import { FOOTAGE_V, FootageClipV } from "./footage"
import { PunchV, TypeCardV } from "./overlays"
import {
  CAMERA_V_KEYS,
  CODING_NOW_V,
  COPY_V,
  CURSOR_V,
  CURSOR_V_KEYS,
  DETAIL_IN_V,
  OVERLAYS_V,
  ROW_CLICK_V,
  ROW_HOVER_V,
  SEGV,
  START_HOVER_V,
  STATUS_FLIP_V,
  UI_MOUNT_AT,
  UI_UNMOUNT_AT,
} from "./timeline"

const CLAMP = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const

const CENTER_X = WIN.rail + WIN.sidebar
const CENTER_W = WIN.w - CENTER_X
const CONTENT_TOP = WIN.topBar + WIN.dockTabs
const DOCK_H = WIN.dockStrip // the dock never opens in the 15s cut
const PANE_H = WIN.h - CONTENT_TOP - DOCK_H

const TAB_EXP: ChromeTab = { id: "exp142", label: HERO.id, mono: true }

// Portrait camera: same key math as ships/rig Camera, focus lands at (540,960).
const CameraV: React.FC<{ keys: CamKey[]; frame: number; children: React.ReactNode }> = ({
  keys,
  frame,
  children,
}) => {
  const { s, x, y } = camAt(keys, frame)
  return (
    <AbsoluteFill
      style={{
        transformOrigin: "0 0",
        translate: `${540 - s * (x + WIN.x)}px ${960 - s * (y + WIN.y)}px`,
        scale: String(s),
      }}
    >
      {children}
    </AbsoluteFill>
  )
}

const HeroShellV: React.FC<{ frame: number }> = ({ frame }) => {
  const heroStatus: IssueStatus = frame >= STATUS_FLIP_V ? "in_progress" : "todo"
  const overrides: Record<string, Partial<BoardRow>> = { [HERO.id]: { status: heroStatus } }
  return (
    <>
      <TopBar frame={frame} />
      <IconRail frame={frame} active="issues" />
      <TabsBar
        frame={frame}
        tabs={frame < DETAIL_IN_V ? [] : [TAB_EXP]}
        activeId="exp142"
        popAt={{ exp142: DETAIL_IN_V }}
      />
      <SidebarPane title="All Issues" actions={<BoardActions />} pills bottomInset={DOCK_H}>
        <BoardTool
          frame={frame}
          rows={BOARD}
          overrides={overrides}
          hover={{ id: HERO.id, from: ROW_HOVER_V.from, to: ROW_HOVER_V.to }}
          selectedId={frame >= ROW_CLICK_V ? HERO.id : undefined}
          showLabels={false}
        />
      </SidebarPane>
      {frame < DETAIL_IN_V + 8 ? (
        <div style={{ opacity: interpolate(frame, [DETAIL_IN_V, DETAIL_IN_V + 6], [1, 0], CLAMP) }}>
          <CenterEmptyState frame={frame} bottom={DOCK_H} />
        </div>
      ) : null}
      {frame >= DETAIL_IN_V ? (
        <div
          style={{
            position: "absolute",
            left: CENTER_X,
            top: CONTENT_TOP,
            width: CENTER_W,
            height: PANE_H,
            overflow: "hidden",
          }}
        >
          <IssueDetailPane
            frame={frame}
            status={heroStatus}
            slideInAt={DETAIL_IN_V}
            staggerAt={DETAIL_IN_V + 6}
            startHover={{ at: START_HOVER_V.at, out: START_HOVER_V.out }}
            codingNow={{ at: CODING_NOW_V, out: UI_UNMOUNT_AT + 10 }}
            width={CENTER_W}
            height={PANE_H}
          />
        </div>
      ) : null}
      <DockCollapsedStrip frame={frame} count={1} />
      <CursorLayer
        keys={CURSOR_V_KEYS}
        clicks={[...CURSOR_V.clicks]}
        frame={frame}
        from={CURSOR_V.from}
        to={CURSOR_V.to}
      />
    </>
  )
}

export const SpotVertical: React.FC = () => {
  const frame = useCurrentFrame() // global (Sequence from=0)

  return (
    <AbsoluteFill>
      {/* UI world — mounts/unmounts under the opaque card plateaus */}
      {frame >= UI_MOUNT_AT && frame < UI_UNMOUNT_AT ? (
        <CameraV keys={CAMERA_V_KEYS} frame={frame}>
          <WindowChassis>
            <HeroShellV frame={frame} />
          </WindowChassis>
        </CameraV>
      ) : null}

      {/* live-action layers (square takes, portrait crop) */}
      <FootageClipV
        spec={FOOTAGE_V.chaos}
        from={SEGV.liveA}
        duration={SEGV.card1 + 10}
        // LOUD café (the take is quiet, ×2 like the 16:9 cut), dying under card1
        volume={(f) => interpolate(f, [0, 46, 78], [2, 2, 0], CLAMP)}
      />
      <FootageClipV
        spec={FOOTAGE_V.payoff}
        from={SEGV.liveC}
        duration={SEGV.outro - SEGV.liveC}
        fadeOut={8}
        // shown window 0.7–3.2s — far from the actor's line at ~6.5s
        volume={() => 1}
      />

      {/* type — the hook punch, two cards, the payoff punch */}
      <PunchV frame={frame} in={OVERLAYS_V.liveA.in} out={OVERLAYS_V.liveA.out} lines={[COPY_V.liveA]} size={60} />
      <TypeCardV frame={frame} in={OVERLAYS_V.card1.in} out={OVERLAYS_V.card1.out} brand>
        <div>{COPY_V.hook1}</div>
        <div>{COPY_V.hook2}</div>
      </TypeCardV>
      <TypeCardV frame={frame} in={OVERLAYS_V.card3.in} out={OVERLAYS_V.card3.out}>
        {COPY_V.card3}
      </TypeCardV>
      <PunchV frame={frame} in={OVERLAYS_V.liveC.in} out={OVERLAYS_V.liveC.out} lines={[COPY_V.liveC]} size={68} />
    </AbsoluteFill>
  )
}
