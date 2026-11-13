// `@exponential_app/eui/web/motion`: the Motion, Canvas and VoiceWave web
// runtime (the renderers' natives import it). Its own entry so `/web` (the
// structural CSS, the data layer, Markdown) does not carry the Motion player,
// the Canvas painter and the wasm glue; until this entry they lived on `/web`.

// NEW-77: the VoiceWave natives' runtime helpers (the primitives barrel would pull every recipe into the standalone bundle).
export { voiceWaveAnimates, voiceWaveClock, voiceWaveFitChild, voiceWaveName } from "./primitives/voice-wave"
export { MotionView, MotionStateDriver, motionA11y, motionBoxStyle, motionCanPaint, motionDeclaredSize, motionDotLottie, motionFit, motionFromDotLottie, motionThemeSlots, paintMotionFrame, motionFitMatrix, motionConcat, motionFrameSize, MotionCanvasPool, MOTION_MAX_PLAYING, MOTION_MAX_FPS } from "./primitives/motion"
export type { MotionEnv, MotionEmit, MotionEvent, MotionMachinePlayer, MotionFit, MotionMatrix } from "./primitives/motion"
export { CanvasView, canvasCanPaint, canvasThemeFonts, resetCanvasThemeIds } from "./primitives/canvas"
export type { CanvasEnv } from "./primitives/canvas"
export { paintText, MOTION_DEFAULT_FONTS, MOTION_PACKED_VERSION } from "./primitives/motion-paint"
export type { MotionTextFonts } from "./primitives/motion-paint"
