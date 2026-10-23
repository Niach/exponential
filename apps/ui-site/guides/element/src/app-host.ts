// A declarative app = templates + data + bindings + the functions it may
// call. Data, never code: validate it, then run it in any host.
import { createAppHost, validatePackage } from "@exponential-at/ui"
import type { AppPackage } from "@exponential-at/ui"
import pkg from "./hello.app.json"

const issues = validatePackage(pkg)
if (issues.length) throw new Error(issues.map((i) => `${i.path}: ${i.message}`).join(`\n`))

export const app = createAppHost({
  package: pkg as AppPackage,
  // The host lends what the package names: the toast (its `functions` list
  // narrows the policy) and the `weather:` source its binding follows.
  functions: { "app.toast": ({ text }) => console.info(String(text)) },
  sources: { weather: (_source, emit) => emit({ summary: `Sunny, 21 °C` }) },
})

app.open(`hello`) // applyTemplate: the first template onto surface "hello"
// Paint it: el.host = app.host on <exponential-surface surface-id="hello" theme="neutral">
