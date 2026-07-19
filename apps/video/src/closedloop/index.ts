// closedloop/index.ts — the ONLY module the marketing app imports (EXP-136:
// the landing page embeds the composition inline via @remotion/player).

export { ClosedLoop } from "./ClosedLoop";
export {
  CHAPTERS,
  DURATION_IN_FRAMES,
  END_HOLD,
  FPS,
  STORY_FRAMES,
} from "./timeline";
