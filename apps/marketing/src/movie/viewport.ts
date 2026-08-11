// The ONE phone breakpoint the film knows about (EXP-392; portrait cut
// EXP-482).
//
// Four things have to agree on it or the handoff visibly jumps: the poster
// <source> in LoopMovie, the `portrait` inputProp + composition dims
// LoopMoviePlayer feeds the Player, and loop.css's full-bleed block with its
// portrait stage aspect-ratio. The first two live here; the stylesheet
// mirrors the same 720px in its media queries.
//
// Like ./closedloop/chapters, this module is imported by the SSR-rendered
// LoopMovie — it must stay import-free and remotion-free forever.
export const SMALL_MEDIA = `(max-width: 720px)`
