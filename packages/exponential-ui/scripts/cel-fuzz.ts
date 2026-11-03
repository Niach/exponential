// The CEL differential fuzz corpus (src/cel-fuzz.ts): `--write` regenerates
// fixtures/cel-fuzz.json; `--seed <n> --count <n> --out <file>` writes an
// exploration corpus for `CEL_FUZZ_CORPUS=<file> cargo test -p eui-core --test cel_fuzz`.

import { CORPUS_COUNT, CORPUS_SEED, generateCorpus } from "../src/cel-fuzz"

const arg = (name: string) => {
  const i = process.argv.indexOf(`--${name}`)
  return i < 0 ? undefined : process.argv[i + 1]
}
const write = process.argv.includes(`--write`)
const seed = Number(arg(`seed`) ?? CORPUS_SEED)
const count = Number(arg(`count`) ?? CORPUS_COUNT)
const out = write ? new URL(`../fixtures/cel-fuzz.json`, import.meta.url).pathname : arg(`out`)
if (!out) throw new Error(`--write or --out <file>`)
const corpus = generateCorpus(seed, count)
// One case per line: a diff names the cases that changed.
const lines = (xs: unknown[]) => `[\n${xs.map((x) => `  ${JSON.stringify(x)}`).join(`,\n`)}\n ]`
const head = { $comment: corpus.$comment, seed: corpus.seed, zones: corpus.zones }
const text = `{\n${Object.entries(head).map(([k, v]) => ` ${JSON.stringify(k)}: ${JSON.stringify(v)}`).join(`,\n`)},\n "varSets": ${lines(corpus.varSets)},\n "cases": ${lines(corpus.cases)},\n "calls": ${lines(corpus.calls)}\n}\n`
JSON.parse(text)
await Bun.write(out, text)
const errors = corpus.cases.filter((c) => `error` in c.eval).length
const checkErrors = corpus.cases.filter((c) => `error` in c.check).length
console.log(`${out}: ${corpus.cases.length} cases (${errors} eval errors, ${checkErrors} check errors), ${corpus.calls.length} calls`)
