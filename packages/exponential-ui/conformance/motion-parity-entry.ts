// EXP-1604: the browser entry of conformance/motion-parity.ts.
import * as painter from "../src/web/primitives/motion-paint"
;(globalThis as unknown as { __motionPaint: typeof painter }).__motionPaint = painter
