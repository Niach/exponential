// Swap `react-native` (bun auto-installs it as react-strict-dom's peer, and
// its Flow source does not parse) and the two deep imports react-strict-dom's
// native build makes for the shim. Preloaded via bunfig.toml. Bun's runtime
// `onResolve` did not intercept bare imports from inside node_modules, so this
// hooks `onLoad` on the resolved files instead.
import { plugin } from "bun"
import { resolve } from "node:path"

;(globalThis as any).__DEV__ = true
const shim = resolve(import.meta.dir, `react-native.ts`)
const stub = resolve(import.meta.dir, `rn-deep-stub.ts`)

plugin({
  name: `react-native-shim`,
  setup(build) {
    build.onLoad({ filter: /node_modules\/react-native\/index\.js$/ }, () => ({
      contents: `export * from ${JSON.stringify(shim)}; export { default } from ${JSON.stringify(shim)};`,
      loader: `js`,
    }))
    build.onLoad({ filter: /node_modules\/react-native\/Libraries\/.*\.js$/ }, () => ({
      contents: `export { default } from ${JSON.stringify(stub)};`,
      loader: `js`,
    }))
  },
})
