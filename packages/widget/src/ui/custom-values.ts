// Custom-field values live in plain objects keyed by owner-authored keys.
// The server's key pattern admits prototype names ("Constructor" slugifies to
// `constructor`), and a plain `values[key]` lookup resolves those to
// prototype-INHERITED functions — the input pre-fills with garbage and
// `.trim()` throws in the submit handler. Every keyed read goes through this
// own-property guard instead. (`hasOwnProperty.call`, not `Object.hasOwn` —
// the bundle targets es2019 and vite transpiles syntax only, never APIs.)
export function ownCustomValue(
  values: Record<string, string>,
  key: string
): string {
  return Object.prototype.hasOwnProperty.call(values, key) ? values[key] : ``
}
