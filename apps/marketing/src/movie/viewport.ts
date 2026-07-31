// The ONE phone breakpoint the film knows about (EXP-392).
//
// Three things have to agree on it or the handoff visibly jumps: the poster
// <source> in LoopMovie, the `small` inputProp LoopMoviePlayer feeds the
// composition, and loop.css's full-bleed block. The first two live here; the
// stylesheet mirrors the same 720px in its media queries.
//
// Like ./closedloop/chapters, this module is imported by the SSR-rendered
// LoopMovie — it must stay import-free and remotion-free forever.
export const SMALL_MEDIA = `(max-width: 720px)`
