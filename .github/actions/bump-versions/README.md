# bump-versions

EXP-1430: rewrites the committed release versions (marketing version + build
numbers) from a JSON config and prints the tags to push. Dependency-free Node
ESM (`bump.mjs`), so it runs on any runner and from any repo of the owner.
It never commits, tags or releases.

## Config

```json
{
  "products": {
    "<name>": {
      "tags": ["<name>-v{version}"],
      "fields": [
        { "file": "path/from/root", "pattern": "^version = \"(?<value>[^\"]+)\"$" },
        { "file": "path", "pattern": "^\\s*versionCode = (?<value>\\d+)$", "kind": "build", "name": "versionCode", "step": 1 },
        { "file": "path", "pattern": "\"[^\"@]+@(?<value>{current})\"", "all": true }
      ],
      "cargoLock": "path/to/Cargo.lock",
      "versionFromTags": "v"
    }
  }
}
```

- `tags`: templates, `{version}` = the new version. Default `<name>-v{version}`.
- `fields[]`: `pattern` is a JS regex (default flags `m`, override with `flags`)
  with ONE named group `value`, the only span rewritten.
  - `kind: "version"` (default): x.y.z. Every version field of a product must
    hold the same current version.
  - `kind: "build"`: an integer, `+ step` (default 1). `name` keys it in the
    `versions` output.
  - Exactly one match, or the run fails. `"all": true` rewrites every match
    (at least one). `{current}` in a pattern = the escaped current version, for
    files listing many `name@version` rows (such a field checks, never
    discovers, the version).
- `cargoLock`: file (or list). Every `[[package]]` without a `source =` line
  (a workspace member) still on the old version moves to the new one. Cargo
  never runs.
- `versionFromTags`: a prefix; the current version is the newest
  `<prefix>x.y.z` git tag (for tag-only products; checkout with
  `fetch-depth: 0`).

A request is `patch`, `minor`, `major` or an explicit `x.y.z` (must be above
the current one). `none` or empty skips the product. Nothing is written unless
every requested product succeeds.

Exponential's own config: `.github/release-versions.json` (desktop + CLI share
the Cargo workspace version; iOS build `step: 2`, since staging takes
build+1).

## CLI

```bash
node .github/actions/bump-versions/bump.mjs --config .github/release-versions.json \
  [--root .] [--dry-run] desktop=patch android=0.14.75 ios=minor
bun run bump:versions -- --dry-run desktop=patch     # the same, from the root
node --test .github/actions/bump-versions/bump.test.mjs
```

stderr = the plan (old/new line per field), stdout = the tags, one per line.
In Actions (`$GITHUB_OUTPUT` set) also the outputs `tags` (multiline),
`versions` (JSON `{product: {version, <build name>: n}}`) and `files`.
Exit 1 with `bump-versions: <reason>` on any failure.

## Action

```yaml
- uses: actions/checkout@v4
  with:
    fetch-depth: 0   # only for versionFromTags
- id: bump
  uses: Niach/exponential/.github/actions/bump-versions@<sha>
  with:
    config: .github/release-versions.json   # default
    bumps: desktop=patch ios=minor
    dry-run: 'false'
- run: echo "${{ steps.bump.outputs.tags }}"
```

From spezl (`Niach/spezl`), its own config in its own repo:

```yaml
jobs:
  bump:
    runs-on: ubuntu-latest
    permissions: { contents: write, pull-requests: write }
    steps:
      - uses: actions/checkout@v4
      - id: bump
        uses: Niach/exponential/.github/actions/bump-versions@<sha>
        with:
          bumps: desktop=${{ inputs.desktop }}
      # commit + gh pr create: copy the last step of
      # Niach/exponential/.github/workflows/bump-versions.yml
```

`.github/workflows/bump-versions.yml` is the full flow: dispatch inputs per
product, commit on `release/bump-<date>-<run>`, PR via `gh`. A PR opened with
`GITHUB_TOKEN` runs no workflow (no `gate`); set a `RELEASE_BOT_TOKEN` secret
(PAT or App token, contents + pull-requests write) to get the checks, otherwise
the run warns and someone pushes to the branch. Tags stay a person's step
after the merge.
