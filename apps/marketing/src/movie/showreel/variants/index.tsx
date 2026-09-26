// showreel/variants/index.tsx — the three variations of the reel (EXP-1100
// r2), each a full 15-second cut with its own direction: Type (kinetic
// typography on a beat), Loop (one continuous orbiting shot) and Blueprint
// (a monospace technical drawing). Same duration and canvas as the main
// reel; Root.tsx registers them as `Showreel-<id>`.

import React from "react"
import { useCurrentFrame } from "remotion"
import { useShowreelFonts } from "../fonts"
import { TypeReel } from "./TypeReel"
import { LoopReel } from "./LoopReel"
import { BlueprintReel } from "./BlueprintReel"

const wrap = (Reel: React.FC<{ f: number }>): React.FC => {
  const Wrapped: React.FC = () => {
    useShowreelFonts()
    const f = useCurrentFrame()
    return <Reel f={f} />
  }
  return Wrapped
}

export const VARIANTS: { id: string; component: React.FC }[] = [
  { id: `Type`, component: wrap(TypeReel) },
  { id: `Loop`, component: wrap(LoopReel) },
  { id: `Blueprint`, component: wrap(BlueprintReel) },
]
