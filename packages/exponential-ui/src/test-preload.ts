// VAPP-106: bunfig.toml preloads this before every test file, so the tests
// see the catalog WITH its descriptions and examples (the way the prompt and
// the schema entries do); the runtime catalog the root entry ships has
// neither (src/catalog.described.ts).
import { describeCatalog } from "./catalog.described"

describeCatalog()
