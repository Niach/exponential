import { describe, expect, it } from "vitest"
import {
  formatArgvLine,
  isBlockedEnvKey,
  parseArgvLine,
  runConfigCwdError,
  sanitizeRunConfigEnv,
} from "@/lib/run-configs"

describe(`runConfigCwdError`, () => {
  it(`accepts relative paths inside the checkout`, () => {
    expect(runConfigCwdError(`apps/web`)).toBeNull()
    expect(runConfigCwdError(`packages/db-schema/src`)).toBeNull()
    expect(runConfigCwdError(`.`)).toBeNull()
    // `..` as a substring of a real name is fine — only whole segments count.
    expect(runConfigCwdError(`apps/..web/dist..`)).toBeNull()
  })

  it(`rejects absolute paths`, () => {
    expect(runConfigCwdError(`/etc`)).toMatch(/relative/)
    expect(runConfigCwdError(`/`)).toMatch(/relative/)
    expect(runConfigCwdError(`\\\\server\\share`)).toMatch(/relative/)
    expect(runConfigCwdError(`C:\\repo`)).toMatch(/relative/)
    expect(runConfigCwdError(`c:/repo`)).toMatch(/relative/)
  })

  it(`rejects ".." segments anywhere in the path`, () => {
    expect(runConfigCwdError(`..`)).toMatch(/\.\./)
    expect(runConfigCwdError(`../sibling`)).toMatch(/\.\./)
    expect(runConfigCwdError(`apps/../..`)).toMatch(/\.\./)
    expect(runConfigCwdError(`apps\\..\\web`)).toMatch(/\.\./)
  })
})

describe(`env sanitization`, () => {
  it(`blocks PATH, LD_PRELOAD and DYLD_* case-insensitively`, () => {
    expect(isBlockedEnvKey(`PATH`)).toBe(true)
    expect(isBlockedEnvKey(`Path`)).toBe(true)
    expect(isBlockedEnvKey(`LD_PRELOAD`)).toBe(true)
    expect(isBlockedEnvKey(`DYLD_INSERT_LIBRARIES`)).toBe(true)
    expect(isBlockedEnvKey(`dyld_library_path`)).toBe(true)
    expect(isBlockedEnvKey(`NODE_ENV`)).toBe(false)
    // Not blocked: merely containing the blocked names.
    expect(isBlockedEnvKey(`MY_PATH`)).toBe(false)
    expect(isBlockedEnvKey(`PATHS`)).toBe(false)
  })

  it(`strips blocked keys and keeps the rest`, () => {
    expect(
      sanitizeRunConfigEnv({
        PATH: `/evil`,
        LD_PRELOAD: `/evil.so`,
        DYLD_INSERT_LIBRARIES: `/evil.dylib`,
        NODE_ENV: `development`,
        PORT: `5173`,
      })
    ).toEqual({ NODE_ENV: `development`, PORT: `5173` })
  })
})

describe(`parseArgvLine`, () => {
  it(`splits on whitespace`, () => {
    expect(parseArgvLine(`bun run dev`)).toEqual([`bun`, `run`, `dev`])
    expect(parseArgvLine(`  bun   run\tdev  `)).toEqual([`bun`, `run`, `dev`])
    expect(parseArgvLine(``)).toEqual([])
    expect(parseArgvLine(`   `)).toEqual([])
  })

  it(`groups quoted arguments`, () => {
    expect(parseArgvLine(`echo "hello world"`)).toEqual([`echo`, `hello world`])
    expect(parseArgvLine(`echo 'single quoted'`)).toEqual([
      `echo`,
      `single quoted`,
    ])
    expect(parseArgvLine(`echo ""`)).toEqual([`echo`, ``])
    expect(parseArgvLine(`echo a"b c"d`)).toEqual([`echo`, `ab cd`])
  })

  it(`handles escapes`, () => {
    expect(parseArgvLine(`echo "a \\"quote\\""`)).toEqual([`echo`, `a "quote"`])
    expect(parseArgvLine(`echo "back\\\\slash"`)).toEqual([
      `echo`,
      `back\\slash`,
    ])
    expect(parseArgvLine(`echo spaced\\ arg`)).toEqual([`echo`, `spaced arg`])
  })

  it(`is forgiving about an unterminated quote`, () => {
    expect(parseArgvLine(`echo "unterminated rest`)).toEqual([
      `echo`,
      `unterminated rest`,
    ])
  })
})

describe(`formatArgvLine`, () => {
  it(`keeps plain args bare and quotes the rest`, () => {
    expect(formatArgvLine([`bun`, `run`, `dev`])).toBe(`bun run dev`)
    expect(formatArgvLine([`echo`, `hello world`])).toBe(`echo "hello world"`)
    expect(formatArgvLine([`echo`, ``])).toBe(`echo ""`)
    expect(formatArgvLine([`echo`, `a "quote"`])).toBe(`echo "a \\"quote\\""`)
  })

  it(`round-trips through parseArgvLine`, () => {
    const cases: string[][] = [
      [`bun`, `run`, `dev`],
      [`echo`, `hello world`, ``],
      [`sh`, `-c`, `echo "nested" && ls 'dir with spaces'`],
      [`printf`, `back\\slash`, `tab\there`],
    ]
    for (const argv of cases) {
      expect(parseArgvLine(formatArgvLine(argv))).toEqual(argv)
    }
  })
})
