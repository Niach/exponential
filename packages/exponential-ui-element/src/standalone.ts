// FEED-130: the standalone entry, bundled self-contained (Solid, the web
// layer and the renderer inlined) into one ES module a page loads with a
// `<script type="module">`: it registers `<exponential-surface>` and
// re-exports the package.
import "./define"

export * from "./index"
