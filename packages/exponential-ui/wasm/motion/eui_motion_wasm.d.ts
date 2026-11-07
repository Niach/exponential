/* tslint:disable */
/* eslint-disable */

/**
 * Round 8: a compiled Canvas (`{size, shapes}`); `sample(slots)` returns
 * the packed frame (shapes, then the `text` ops).
 */
export class CanvasPlayer {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Compile a Canvas node's `{size, shapes}` JSON. Throws a JSON array of
     * issues when refused.
     */
    constructor(source: string);
    /**
     * The packed frame with these slot overrides (JSON object, or "").
     */
    sample(slots: string): Float32Array;
    /**
     * Every slot the canvas declares (`$color.<token>`, `ld:<light>:<dark>`):
     * the ids a host fills from its theme, exactly like a Motion's.
     * A JSON array of strings (a string crosses cheaper than a `string[]`).
     */
    slotIds(): string;
}

/**
 * A loaded animation; `sample(frame)` returns the packed frame.
 */
export class MotionPlayer {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Register a font (TTF/OTF bytes) for text layers without glyphs.
     */
    addFont(family: string, data: Uint8Array): void;
    /**
     * Whether a finite loop count has played out at `elapsedMs`.
     */
    done(elapsed_ms: number): boolean;
    frameRate(): number;
    height(): number;
    /**
     * Image asset locations, in `Image` op order.
     */
    images(): string[];
    inPoint(): number;
    /**
     * Markers as JSON `[{name, time, duration}]` (frames).
     */
    markers(): string;
    needsFonts(): boolean;
    /**
     * Load Lottie JSON (`slots` = a JSON object of slot overrides, or "").
     * Throws a JSON array of issues when refused.
     */
    constructor(source: string, slots: string);
    outPoint(): number;
    /**
     * The still frame (reduced motion, paused, before play).
     */
    posterFrame(): number;
    /**
     * The frame a bound progress (0–1) shows.
     */
    progressFrame(progress: number): number;
    /**
     * Sample `frame` into the packed f32 encoding (a copy into JS memory).
     */
    sample(frame: number): Float32Array;
    /**
     * Set the playback props (JSON: autoplay, loop, direction, speed, segment, poster).
     */
    setPlayback(json: string): void;
    /**
     * Start a dotLottie state machine (JSON); its current state's playback
     * replaces `setPlayback`. Returns false when it has no states.
     */
    setStateMachine(json: string): boolean;
    /**
     * Report an interaction (click, pointerDown, pointerUp, pointerEnter,
     * pointerExit, complete, loopComplete; `layer` = the hit layer or "").
     * Returns whether the state changed (restart the clock).
     */
    smPost(kind: string, layer: string): boolean;
    /**
     * Write state-machine inputs (JSON object); returns whether the state changed.
     */
    smSetInputs(json: string): boolean;
    /**
     * `{state, entered: [...], final, inputs}`; `entered` drains (→ `on.state` actions).
     */
    smStatus(): string;
    /**
     * The frame `elapsedMs` after play started; NaN-free. `done()` tells a finished finite loop.
     */
    tick(elapsed_ms: number): number;
    /**
     * The non-fatal findings as a JSON array.
     */
    warnings(): string;
    width(): number;
}

/**
 * Compile a dialect document: `{"lottie": {…}}` or `{"errors": […]}`.
 */
export function compile(source: string): string;

/**
 * Validate a Lottie JSON string: `{"errors":[…],"warnings":[…]}`.
 */
export function validate(source: string): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_canvasplayer_free: (a: number, b: number) => void;
    readonly __wbg_motionplayer_free: (a: number, b: number) => void;
    readonly canvasplayer_new: (a: number, b: number) => [number, number, number];
    readonly canvasplayer_sample: (a: number, b: number, c: number) => any;
    readonly canvasplayer_slotIds: (a: number) => [number, number];
    readonly compile: (a: number, b: number) => [number, number];
    readonly motionplayer_addFont: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly motionplayer_done: (a: number, b: number) => number;
    readonly motionplayer_frameRate: (a: number) => number;
    readonly motionplayer_height: (a: number) => number;
    readonly motionplayer_images: (a: number) => [number, number];
    readonly motionplayer_inPoint: (a: number) => number;
    readonly motionplayer_markers: (a: number) => [number, number];
    readonly motionplayer_needsFonts: (a: number) => number;
    readonly motionplayer_new: (a: number, b: number, c: number, d: number) => [number, number, number];
    readonly motionplayer_outPoint: (a: number) => number;
    readonly motionplayer_posterFrame: (a: number) => number;
    readonly motionplayer_progressFrame: (a: number, b: number) => number;
    readonly motionplayer_sample: (a: number, b: number) => any;
    readonly motionplayer_setPlayback: (a: number, b: number, c: number) => void;
    readonly motionplayer_setStateMachine: (a: number, b: number, c: number) => number;
    readonly motionplayer_smPost: (a: number, b: number, c: number, d: number, e: number) => number;
    readonly motionplayer_smSetInputs: (a: number, b: number, c: number) => number;
    readonly motionplayer_smStatus: (a: number) => [number, number];
    readonly motionplayer_tick: (a: number, b: number) => number;
    readonly motionplayer_warnings: (a: number) => [number, number];
    readonly motionplayer_width: (a: number) => number;
    readonly validate: (a: number, b: number) => [number, number];
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __externref_drop_slice: (a: number, b: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
