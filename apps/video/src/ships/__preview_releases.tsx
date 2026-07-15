// TEMPORARY preview harness for surfaces/releases.tsx — deleted after visual check.
import React from "react"
import { Composition, registerRoot, useCurrentFrame } from "remotion"
import { C } from "./theme"
import { ReleasesTool, ReleaseDetailTool } from "./surfaces/releases"

const Preview: React.FC = () => {
  const frame = useCurrentFrame()
  return (
    <div style={{ width: 600, height: 960, backgroundColor: C.canvas, display: "flex", gap: 20, padding: 20 }}>
      <div style={{ position: "relative", width: 260, height: 900, backgroundColor: C.bg, border: `1px solid ${C.border}` }}>
        <ReleasesTool frame={frame} hover={{ at: 40, out: 70 }} />
      </div>
      <div style={{ position: "relative", width: 260, height: 900, backgroundColor: C.bg, border: `1px solid ${C.border}` }}>
        <ReleaseDetailTool
          frame={frame}
          drillAt={10}
          progress={[
            { at: 60, from: 3, to: 4, dur: 6 },
            { at: 75, from: 4, to: 5, dur: 6 },
            { at: 90, from: 5, to: 7, dur: 6 },
            { at: 130, from: 7, to: 8, dur: 12 },
          ]}
          statusFlipAt={{ "EXP-139": 62, "EXP-141": 77 }}
          shippedAt={150}
          cascadeDoneAt={160}
          hoverStartCoding={{ at: 30, out: 55 }}
        />
      </div>
    </div>
  )
}

registerRoot(() => (
  <Composition id="RelPreview" component={Preview} durationInFrames={240} fps={30} width={600} height={960} />
))
