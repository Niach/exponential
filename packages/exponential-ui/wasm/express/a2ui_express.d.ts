/* tslint:disable */
/* eslint-disable */

/** A catalog document (A2UI catalog JSON). */
export type CatalogDocument = Record<string, unknown>;
/** One catalog document, or a list of them. */
export type CatalogDocuments = CatalogDocument | CatalogDocument[];
/** An A2UI message, as compiled (`createSurface`, `updateComponents`, …). */
export type A2uiMessage = Record<string, unknown>;
/** The error categories: the suite's two, plus catalog and argument errors. */
export type ErrorCategory = "ParseError" | "ValidationError" | "CatalogError" | "ArgumentError";
/** A part of a parsed response. */
export type Part =
| { text: string }
| { a2ui: A2uiMessage[] }
| { error: { category: ErrorCategory; message: string } };
/** A part of a response before its blocks are compiled. */
export type RawPart = { text: string } | { a2ui_raw: string; is_final?: boolean };
/** What every call throws: an `Error` whose `name` is its category. */
export interface ExpressError extends Error {
    category: ErrorCategory;
    /** Parts made ready before the failure (response reads only; else empty). */
    partial: Part[];
}
export interface CompileOptions {
    /** The surface a block without `surface(...)` targets (default `default_surface`). */
    surfaceId?: string;
    /** The protocol version, `"1.0"` or `"0.9"` (default: the catalogs'). */
    version?: string;
}
export interface ResponseOptions {
    /** The surface a block without `surface(...)` targets (default `default_surface`). */
    surfaceId?: string;
    /** `false` reads the whole response as one block without `<a2ui>` tags (default `true`). */
    wrapped?: boolean;
    /** Keys whose open strings heal while streaming (default `["text"]`; `[]` turns it off). */
    progressiveKeys?: string[];
    /** After a rejected block: `"end"` throws (default), `"continue"` yields an error part and reads on. */
    afterRejectedBlock?: "end" | "continue";
    /** The protocol version, `"1.0"` or `"0.9"` (default: the catalogs'). */
    version?: string;
}

/** Compiles one Express block (`<a2ui>` tags optional) to A2UI messages. */
export function compile(catalogs: CatalogDocuments, source: string, options?: CompileOptions): A2uiMessage[];
/** Reads a whole response (prose with `<a2ui>` blocks) and compiles each block. */
export function parseResponse(catalogs: CatalogDocuments, text: string, options?: ResponseOptions): Part[];
/** Splits a response into text and raw block parts. */
export function unwrap(text: string): RawPart[];
/** Writes raw parts back out as a model would have emitted them. */
export function wrap(parts: RawPart[]): string;
/** Whether a response holds an `<a2ui>` block (`complete`: a closed one). */
export function hasFormatContent(text: string, complete?: boolean): boolean;

export interface CatalogsOptions {
    /** The protocol version, `"1.0"` or `"0.9"` (default: the catalogs'). */
    version?: string;
    /**
     * The surface id streams and responses mostly use (default `default_surface`):
     * its parser is set up with the catalogs. Another surface id's is set up on
     * first use, which costs about what loading the catalogs does.
     */
    surfaceId?: string;
}

/**
 * Loaded catalogs: a catalog is read once, then compiled against any number of
 * times. Prefer it to the free functions for a large catalog (loading eui's
 * catalog takes ~70 ms; compiling a block against it, about a millisecond per
 * statement). Options' `version`, if given, must be the one loaded.
 */
export class Catalogs {
    constructor(catalogs: CatalogDocuments, options?: CatalogsOptions);
    /** The protocol version messages are compiled for (`"1.0"`, `"v0.9"`). */
    readonly version: string;
    /** The catalog ids, in order. */
    readonly ids: string[];
    compile(source: string, options?: CompileOptions): A2uiMessage[];
    parseResponse(text: string, options?: ResponseOptions): Part[];
    /** Opens a `StreamCompiler` on these catalogs. */
    stream(options?: ResponseOptions): StreamCompiler;
    free(): void;
    [Symbol.dispose](): void;
}

/**
 * Reads a model response as it streams in: each `push` returns the parts that
 * chunk made ready, so a surface renders while the model types. Streamed in
 * any chunk size, a response yields the same surfaces as `parseResponse`.
 */
export class StreamCompiler {
    constructor(catalogs: CatalogDocuments, options?: ResponseOptions);
    /** Feeds a chunk; returns the parts it made ready. Throws an `ExpressError`. */
    push(chunk: string): Part[];
    /** Ends the response; returns what was still held back. Throws an `ExpressError`. */
    finish(): Part[];
    /** Whether the stream is inside an `<a2ui>` block. */
    readonly inBlock: boolean;
    /** Frees the wasm memory now (else the finalizer does). */
    free(): void;
    [Symbol.dispose](): void;
}



export interface DecompileOptions {
    /** Write arguments as `name=value` instead of positionally. */
    keywordArgs?: boolean;
    /** `"upstream"` (message order, default) or `"top-down"` (`root` first, for streaming). */
    order?: "upstream" | "top-down";
}
/** Writes A2UI messages back as one Express block (without tags). */
export function decompile(catalogs: CatalogDocuments, messages: A2uiMessage[], options?: DecompileOptions): string;
export interface Catalogs {
    /** Writes A2UI messages back as one Express block (without tags). */
    decompile(messages: A2uiMessage[], options?: DecompileOptions): string;
}



export interface PromptOptions {
    /** The surface the agent writes to (default `default_surface`). */
    surfaceId?: string;
    /** Examples: one list of A2UI messages per turn, rewritten into Express in the prompt. */
    examples?: A2uiMessage[][];
    /** The message types the agent may write (`createSurface`, `updateComponents`, …); default all. */
    allowedMessages?: string[];
    /**
     * How the signatures are written: `"upstream"` (default, upstream's format byte for byte)
     * or `"compact"` (shared types and common properties listed once; far shorter on large catalogs).
     */
    style?: "upstream" | "compact";
}
/** Generates the system-prompt section that teaches a model Express for the catalogs. */
export function generatePrompt(catalogs: CatalogDocuments, options?: PromptOptions): string;
/** The functions an agent may call standalone (`callRendererFunction`), across the catalogs. */
export function agentCallableFunctions(catalogs: CatalogDocuments): string[];
export interface Catalogs {
    /** The system-prompt section that teaches a model Express for these catalogs. */
    generatePrompt(options?: PromptOptions): string;
    /** The functions an agent may call standalone, across these catalogs. */
    agentCallableFunctions(): string[];
}



export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_catalogs_free: (a: number, b: number) => void;
    readonly __wbg_streamcompiler_free: (a: number, b: number) => void;
    readonly agentCallableFunctions: (a: any) => [number, number, number];
    readonly catalogs_agentCallableFunctions: (a: number) => [number, number];
    readonly catalogs_compile: (a: number, b: number, c: number, d: number) => [number, number, number];
    readonly catalogs_decompile: (a: number, b: any, c: number) => [number, number, number, number];
    readonly catalogs_generatePrompt: (a: number, b: number) => [number, number, number, number];
    readonly catalogs_ids: (a: number) => [number, number];
    readonly catalogs_new: (a: any, b: number) => [number, number, number];
    readonly catalogs_parseResponse: (a: number, b: number, c: number, d: number) => [number, number, number];
    readonly catalogs_stream: (a: number, b: number) => [number, number, number];
    readonly catalogs_version: (a: number) => [number, number];
    readonly compile: (a: any, b: number, c: number, d: number) => [number, number, number];
    readonly decompile: (a: any, b: any, c: number) => [number, number, number, number];
    readonly generatePrompt: (a: any, b: number) => [number, number, number, number];
    readonly hasFormatContent: (a: number, b: number, c: number) => number;
    readonly parseResponse: (a: any, b: number, c: number, d: number) => [number, number, number];
    readonly streamcompiler_finish: (a: number) => [number, number, number];
    readonly streamcompiler_in_block: (a: number) => number;
    readonly streamcompiler_new: (a: any, b: number) => [number, number, number];
    readonly streamcompiler_push: (a: number, b: number, c: number) => [number, number, number];
    readonly unwrap: (a: number, b: number) => any;
    readonly wrap: (a: any) => [number, number, number, number];
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __externref_table_dealloc: (a: number) => void;
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
