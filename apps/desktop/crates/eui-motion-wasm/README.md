# `eui-motion-wasm`

The wasm-bindgen facade over [`eui-motion`](../eui-motion) for the three web
renderers (lazy-loaded) and the server-side validator in `exponential_show`.

```js
import init, { MotionPlayer, validate } from "eui-motion-wasm";
await init();
const player = new MotionPlayer(lottieJson);  // throws a JSON array of issues when refused
const frame = player.sample(12.5);            // Float32Array, the packed display list
validate(lottieJson);                          // '{"errors":[…],"warnings":[…]}'
```

Build (needs `wasm32-unknown-unknown` and wasm-bindgen-cli at the
`Cargo.lock` version):

```bash
cd apps/desktop
cargo build -p eui-motion-wasm --target wasm32-unknown-unknown --profile wasm
wasm-bindgen --target web --no-typescript --out-dir pkg $CARGO_TARGET_DIR/wasm32-unknown-unknown/wasm/eui_motion_wasm.wasm
```
