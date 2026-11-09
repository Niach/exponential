// Test shapes of the repo's two changelog files (#LOGS-2): the web
// `CHANGELOG` array and the desktop mirror's `LATEST` + `PREVIOUS_<N>`.

const tsEntry = (id: string) => `  {\n    id: \`${id}\`,\n    date: \`2026-11-09\`,\n    title: \`${id.toUpperCase()}\`,\n  },\n`

/** The web changelog, newest first. */
export const webChangelog = (ids: string[]) =>
  `// Newest first.\nexport const CHANGELOG: ChangelogEntry[] = [\n${ids.map(tsEntry).join(``)}]\n\nexport const tail = 1\n`

const rsFields = (id: string) => `    id: "${id}",\n    date: "2026-11-09",\n    title: "${id.toUpperCase()}",\n};\n`

/** The desktop mirror: LATEST + the earlier heads, highest N first. */
export const rustChangelog = (latest: string, previous: Array<[number, string]>) =>
  `pub(crate) const LATEST: ChangelogEntry = ChangelogEntry {\n${rsFields(latest)}` +
  previous.map(([n, id]) => `\n/// An earlier head entry.\n#[allow(dead_code)]\nconst PREVIOUS_${n}: ChangelogEntry = ChangelogEntry {\n${rsFields(id)}`).join(``) +
  `\nfn render() {}\n`
