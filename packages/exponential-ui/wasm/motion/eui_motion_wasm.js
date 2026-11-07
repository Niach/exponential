/* @ts-self-types="./eui_motion_wasm.d.ts" */

/**
 * Round 8: a compiled Canvas (`{size, shapes}`); `sample(slots)` returns
 * the packed frame (shapes, then the `text` ops).
 */
export class CanvasPlayer {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        CanvasPlayerFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_canvasplayer_free(ptr, 0);
    }
    /**
     * Compile a Canvas node's `{size, shapes}` JSON. Throws a JSON array of
     * issues when refused.
     * @param {string} source
     */
    constructor(source) {
        const ptr0 = passStringToWasm0(source, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.canvasplayer_new(ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        this.__wbg_ptr = ret[0];
        CanvasPlayerFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * The packed frame with these slot overrides (JSON object, or "").
     * @param {string} slots
     * @returns {Float32Array}
     */
    sample(slots) {
        const ptr0 = passStringToWasm0(slots, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.canvasplayer_sample(this.__wbg_ptr, ptr0, len0);
        return ret;
    }
    /**
     * Every slot the canvas declares (`$color.<token>`, `ld:<light>:<dark>`):
     * the ids a host fills from its theme, exactly like a Motion's.
     * A JSON array of strings (a string crosses cheaper than a `string[]`).
     * @returns {string}
     */
    slotIds() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.canvasplayer_slotIds(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
}
if (Symbol.dispose) CanvasPlayer.prototype[Symbol.dispose] = CanvasPlayer.prototype.free;

/**
 * A loaded animation; `sample(frame)` returns the packed frame.
 */
export class MotionPlayer {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        MotionPlayerFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_motionplayer_free(ptr, 0);
    }
    /**
     * Register a font (TTF/OTF bytes) for text layers without glyphs.
     * @param {string} family
     * @param {Uint8Array} data
     */
    addFont(family, data) {
        const ptr0 = passStringToWasm0(family, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passArray8ToWasm0(data, wasm.__wbindgen_malloc);
        const len1 = WASM_VECTOR_LEN;
        wasm.motionplayer_addFont(this.__wbg_ptr, ptr0, len0, ptr1, len1);
    }
    /**
     * Whether a finite loop count has played out at `elapsedMs`.
     * @param {number} elapsed_ms
     * @returns {boolean}
     */
    done(elapsed_ms) {
        const ret = wasm.motionplayer_done(this.__wbg_ptr, elapsed_ms);
        return ret !== 0;
    }
    /**
     * @returns {number}
     */
    frameRate() {
        const ret = wasm.motionplayer_frameRate(this.__wbg_ptr);
        return ret;
    }
    /**
     * @returns {number}
     */
    height() {
        const ret = wasm.motionplayer_height(this.__wbg_ptr);
        return ret;
    }
    /**
     * Image asset locations, in `Image` op order.
     * @returns {string[]}
     */
    images() {
        const ret = wasm.motionplayer_images(this.__wbg_ptr);
        var v1 = getArrayJsValueFromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
    /**
     * @returns {number}
     */
    inPoint() {
        const ret = wasm.motionplayer_inPoint(this.__wbg_ptr);
        return ret;
    }
    /**
     * Markers as JSON `[{name, time, duration}]` (frames).
     * @returns {string}
     */
    markers() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.motionplayer_markers(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {boolean}
     */
    needsFonts() {
        const ret = wasm.motionplayer_needsFonts(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * Load Lottie JSON (`slots` = a JSON object of slot overrides, or "").
     * Throws a JSON array of issues when refused.
     * @param {string} source
     * @param {string} slots
     */
    constructor(source, slots) {
        const ptr0 = passStringToWasm0(source, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(slots, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.motionplayer_new(ptr0, len0, ptr1, len1);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        this.__wbg_ptr = ret[0];
        MotionPlayerFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * @returns {number}
     */
    outPoint() {
        const ret = wasm.motionplayer_outPoint(this.__wbg_ptr);
        return ret;
    }
    /**
     * The still frame (reduced motion, paused, before play).
     * @returns {number}
     */
    posterFrame() {
        const ret = wasm.motionplayer_posterFrame(this.__wbg_ptr);
        return ret;
    }
    /**
     * The frame a bound progress (0–1) shows.
     * @param {number} progress
     * @returns {number}
     */
    progressFrame(progress) {
        const ret = wasm.motionplayer_progressFrame(this.__wbg_ptr, progress);
        return ret;
    }
    /**
     * Sample `frame` into the packed f32 encoding (a copy into JS memory).
     * @param {number} frame
     * @returns {Float32Array}
     */
    sample(frame) {
        const ret = wasm.motionplayer_sample(this.__wbg_ptr, frame);
        return ret;
    }
    /**
     * Set the playback props (JSON: autoplay, loop, direction, speed, segment, poster).
     * @param {string} json
     */
    setPlayback(json) {
        const ptr0 = passStringToWasm0(json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.motionplayer_setPlayback(this.__wbg_ptr, ptr0, len0);
    }
    /**
     * Start a dotLottie state machine (JSON); its current state's playback
     * replaces `setPlayback`. Returns false when it has no states.
     * @param {string} json
     * @returns {boolean}
     */
    setStateMachine(json) {
        const ptr0 = passStringToWasm0(json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.motionplayer_setStateMachine(this.__wbg_ptr, ptr0, len0);
        return ret !== 0;
    }
    /**
     * Report an interaction (click, pointerDown, pointerUp, pointerEnter,
     * pointerExit, complete, loopComplete; `layer` = the hit layer or "").
     * Returns whether the state changed (restart the clock).
     * @param {string} kind
     * @param {string} layer
     * @returns {boolean}
     */
    smPost(kind, layer) {
        const ptr0 = passStringToWasm0(kind, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(layer, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.motionplayer_smPost(this.__wbg_ptr, ptr0, len0, ptr1, len1);
        return ret !== 0;
    }
    /**
     * Write state-machine inputs (JSON object); returns whether the state changed.
     * @param {string} json
     * @returns {boolean}
     */
    smSetInputs(json) {
        const ptr0 = passStringToWasm0(json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.motionplayer_smSetInputs(this.__wbg_ptr, ptr0, len0);
        return ret !== 0;
    }
    /**
     * `{state, entered: [...], final, inputs}`; `entered` drains (→ `on.state` actions).
     * @returns {string}
     */
    smStatus() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.motionplayer_smStatus(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * The frame `elapsedMs` after play started; NaN-free. `done()` tells a finished finite loop.
     * @param {number} elapsed_ms
     * @returns {number}
     */
    tick(elapsed_ms) {
        const ret = wasm.motionplayer_tick(this.__wbg_ptr, elapsed_ms);
        return ret;
    }
    /**
     * The non-fatal findings as a JSON array.
     * @returns {string}
     */
    warnings() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.motionplayer_warnings(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {number}
     */
    width() {
        const ret = wasm.motionplayer_width(this.__wbg_ptr);
        return ret;
    }
}
if (Symbol.dispose) MotionPlayer.prototype[Symbol.dispose] = MotionPlayer.prototype.free;

/**
 * Compile a dialect document: `{"lottie": {…}}` or `{"errors": […]}`.
 * @param {string} source
 * @returns {string}
 */
export function compile(source) {
    let deferred2_0;
    let deferred2_1;
    try {
        const ptr0 = passStringToWasm0(source, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.compile(ptr0, len0);
        deferred2_0 = ret[0];
        deferred2_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
    }
}

/**
 * Validate a Lottie JSON string: `{"errors":[…],"warnings":[…]}`.
 * @param {string} source
 * @returns {string}
 */
export function validate(source) {
    let deferred2_0;
    let deferred2_1;
    try {
        const ptr0 = passStringToWasm0(source, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.validate(ptr0, len0);
        deferred2_0 = ret[0];
        deferred2_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
    }
}
function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
        __wbg___wbindgen_throw_344f42d3211c4765: function(arg0, arg1) {
            throw new Error(getStringFromWasm0(arg0, arg1));
        },
        __wbg_new_from_slice_ddf8b82c4d6af38e: function(arg0, arg1) {
            const ret = new Float32Array(getArrayF32FromWasm0(arg0, arg1));
            return ret;
        },
        __wbindgen_cast_0000000000000001: function(arg0, arg1) {
            // Cast intrinsic for `Ref(String) -> Externref`.
            const ret = getStringFromWasm0(arg0, arg1);
            return ret;
        },
        __wbindgen_init_externref_table: function() {
            const table = wasm.__wbindgen_externrefs;
            const offset = table.grow(4);
            table.set(0, undefined);
            table.set(offset + 0, undefined);
            table.set(offset + 1, null);
            table.set(offset + 2, true);
            table.set(offset + 3, false);
        },
    };
    return {
        __proto__: null,
        "./eui_motion_wasm_bg.js": import0,
    };
}

const CanvasPlayerFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_canvasplayer_free(ptr, 1));
const MotionPlayerFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_motionplayer_free(ptr, 1));

function getArrayF32FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getFloat32ArrayMemory0().subarray(ptr / 4, ptr / 4 + len);
}

function getArrayJsValueFromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    const mem = getDataViewMemory0();
    const result = [];
    for (let i = ptr; i < ptr + 4 * len; i += 4) {
        result.push(wasm.__wbindgen_externrefs.get(mem.getUint32(i, true)));
    }
    wasm.__externref_drop_slice(ptr, len);
    return result;
}

let cachedDataViewMemory0 = null;
function getDataViewMemory0() {
    if (cachedDataViewMemory0 === null || cachedDataViewMemory0.buffer.detached === true || (cachedDataViewMemory0.buffer.detached === undefined && cachedDataViewMemory0.buffer !== wasm.memory.buffer)) {
        cachedDataViewMemory0 = new DataView(wasm.memory.buffer);
    }
    return cachedDataViewMemory0;
}

let cachedFloat32ArrayMemory0 = null;
function getFloat32ArrayMemory0() {
    if (cachedFloat32ArrayMemory0 === null || cachedFloat32ArrayMemory0.byteLength === 0) {
        cachedFloat32ArrayMemory0 = new Float32Array(wasm.memory.buffer);
    }
    return cachedFloat32ArrayMemory0;
}

function getStringFromWasm0(ptr, len) {
    return decodeText(ptr >>> 0, len);
}

let cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

function passArray8ToWasm0(arg, malloc) {
    const ptr = malloc(arg.length * 1, 1) >>> 0;
    getUint8ArrayMemory0().set(arg, ptr / 1);
    WASM_VECTOR_LEN = arg.length;
    return ptr;
}

function passStringToWasm0(arg, malloc, realloc) {
    if (realloc === undefined) {
        const buf = cachedTextEncoder.encode(arg);
        const ptr = malloc(buf.length, 1) >>> 0;
        getUint8ArrayMemory0().subarray(ptr, ptr + buf.length).set(buf);
        WASM_VECTOR_LEN = buf.length;
        return ptr;
    }

    let len = arg.length;
    let ptr = malloc(len, 1) >>> 0;

    const mem = getUint8ArrayMemory0();

    let offset = 0;

    for (; offset < len; offset++) {
        const code = arg.charCodeAt(offset);
        if (code > 0x7F) break;
        mem[ptr + offset] = code;
    }
    if (offset !== len) {
        if (offset !== 0) {
            arg = arg.slice(offset);
        }
        ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
        const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
        const ret = cachedTextEncoder.encodeInto(arg, view);

        offset += ret.written;
        ptr = realloc(ptr, len, offset, 1) >>> 0;
    }

    WASM_VECTOR_LEN = offset;
    return ptr;
}

function takeFromExternrefTable0(idx) {
    const value = wasm.__wbindgen_externrefs.get(idx);
    wasm.__externref_table_dealloc(idx);
    return value;
}

let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
cachedTextDecoder.decode();
const MAX_SAFARI_DECODE_BYTES = 2146435072;
let numBytesDecoded = 0;
function decodeText(ptr, len) {
    numBytesDecoded += len;
    if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
        cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
        cachedTextDecoder.decode();
        numBytesDecoded = len;
    }
    return cachedTextDecoder.decode(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

const cachedTextEncoder = new TextEncoder();

if (!('encodeInto' in cachedTextEncoder)) {
    cachedTextEncoder.encodeInto = function (arg, view) {
        const buf = cachedTextEncoder.encode(arg);
        view.set(buf);
        return {
            read: arg.length,
            written: buf.length
        };
    };
}

let WASM_VECTOR_LEN = 0;

let wasmModule, wasmInstance, wasm;
function __wbg_finalize_init(instance, module) {
    wasmInstance = instance;
    wasm = instance.exports;
    wasmModule = module;
    cachedDataViewMemory0 = null;
    cachedFloat32ArrayMemory0 = null;
    cachedUint8ArrayMemory0 = null;
    wasm.__wbindgen_start();
    return wasm;
}

async function __wbg_load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);
            } catch (e) {
                const validResponse = module.ok && expectedResponseType(module.type);

                if (validResponse && module.headers.get('Content-Type') !== 'application/wasm') {
                    console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);

                } else { throw e; }
            }
        }

        const bytes = await module.arrayBuffer();
        return await WebAssembly.instantiate(bytes, imports);
    } else {
        const instance = await WebAssembly.instantiate(module, imports);

        if (instance instanceof WebAssembly.Instance) {
            return { instance, module };
        } else {
            return instance;
        }
    }

    function expectedResponseType(type) {
        switch (type) {
            case 'basic': case 'cors': case 'default': return true;
        }
        return false;
    }
}

function initSync(module) {
    if (wasm !== undefined) return wasm;


    if (module !== undefined) {
        if (Object.getPrototypeOf(module) === Object.prototype) {
            ({module} = module)
        } else {
            console.warn('using deprecated parameters for `initSync()`; pass a single object instead')
        }
    }

    const imports = __wbg_get_imports();
    if (!(module instanceof WebAssembly.Module)) {
        module = new WebAssembly.Module(module);
    }
    const instance = new WebAssembly.Instance(module, imports);
    return __wbg_finalize_init(instance, module);
}

async function __wbg_init(module_or_path) {
    if (wasm !== undefined) return wasm;


    if (module_or_path !== undefined) {
        if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
            ({module_or_path} = module_or_path)
        } else {
            console.warn('using deprecated parameters for the initialization function; pass a single object instead')
        }
    }

    if (module_or_path === undefined) {
        module_or_path = new URL('eui_motion_wasm_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports();

    if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
        module_or_path = fetch(module_or_path);
    }

    const { instance, module } = await __wbg_load(await module_or_path, imports);

    return __wbg_finalize_init(instance, module);
}

export { initSync, __wbg_init as default };
