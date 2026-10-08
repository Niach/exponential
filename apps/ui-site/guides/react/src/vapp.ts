// A declarative vapp = templates + data + bindings + the functions it may
// call. Data, never code: validate it, then run it in any host.
import { createVappHost, validatePackage } from "@exponential-at/ui"
import type { VappPackage } from "@exponential-at/ui"
import pkg from "./hello.vapp.json"

const issues = validatePackage(pkg)
if (issues.length) throw new Error(issues.map((i) => `${i.path}: ${i.message}`).join(`\n`))

export const vapp = createVappHost({
  package: pkg as VappPackage,
  // The host lends what the package names: the toast (its `functions` list
  // narrows the policy) and the `weather:` source its binding follows.
  functions: { "app.toast": ({ text }) => console.info(String(text)) },
  sources: { weather: (_source, emit) => emit({ summary: `Sunny, 21 °C` }) },
})

vapp.open(`hello`) // applyTemplate: the first template onto surface "hello"
// Paint it: <HostSurface host={vapp.host} surfaceId="hello" theme="neutral" />
