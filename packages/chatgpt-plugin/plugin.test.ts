import { existsSync, readFileSync, readdirSync } from "node:fs"
import { join, resolve } from "node:path"
import { describe, expect, it } from "vitest"

// EXP-1153: the plugin directory's package checks, run BEFORE the upload.
// Every limit here is the submission limit from
// developers.openai.com/plugins/deploy/submission (Manifest fields) or a
// rule from plugin-guidelines; the portal accepts a longer draft and then
// refuses to submit it, which is the slow way to learn the same thing.

const root = resolve(import.meta.dirname)
const read = (p: string) => readFileSync(join(root, p), `utf8`)

type Manifest = {
  $schema: string
  name: string
  version: string
  description: string
  author: { name: string; email?: string; url?: string }
  extensions: {
    "com.openai": {
      interface: Record<string, unknown> & {
        displayName: string
        shortDescription: string
        longDescription: string
        developerName: string
        category: string
        capabilities: string[]
        websiteURL: string
        supportURL: string
        privacyPolicyURL: string
        termsOfServiceURL: string
        defaultPrompt: string[]
        brandColor: string
        brandColorDark: string
        composerIcon: string
        composerIconDark: string
        logo: string
        screenshots?: string[]
      }
      onboardingSkill: string
      review: {
        test_cases: {
          positive: Array<{
            description: string
            prompt: string
            tools_triggered: string
            expected_behavior: string
          }>
          negative: Array<{ description: string; prompt: string }>
        }
        commerce: boolean
      }
      publication: { countries: string[]; release_notes: string }
    }
  }
}

const manifest = JSON.parse(read(`plugin.json`)) as Manifest
const ui = manifest.extensions[`com.openai`].interface
const review = manifest.extensions[`com.openai`].review

// The MCP tool names the review cases may name — mirrors the server's table
// so a renamed tool fails HERE, not in the portal's scan.
const annotationsSource = readFileSync(
  resolve(root, `../../apps/web/src/lib/mcp/annotations.ts`),
  `utf8`
)
const knownTools = new Set(
  [...annotationsSource.matchAll(/^\s+(exponential_[a-z_]+):/gm)].map(
    (m) => m[1]
  )
)

// Words the guidelines forbid in listing copy: no pricing, promotions or
// comparisons, and no "MCP"/"Plugin" glued onto the product name.
const FORBIDDEN_COPY = /\b(free|pricing|price|trial|discount|subscription|\$|€|better than|unlike)\b/i

describe(`plugin.json (Agent Plugins format)`, () => {
  it(`declares the schema and a lowercase package name`, () => {
    expect(manifest.$schema).toBe(
      `https://agent-plugins.org/schemas/1.0.0/plugin.schema.json`
    )
    expect(manifest.name).toMatch(/^[a-z0-9]+(-[a-z0-9]+)*$/)
    expect(manifest.name.length).toBeLessThanOrEqual(64)
    expect(manifest.version).toMatch(/^\d+\.\d+\.\d+$/)
    expect(manifest.description.length).toBeLessThanOrEqual(4000)
    expect(manifest.author.name.length).toBeLessThanOrEqual(120)
  })

  it(`keeps the listing inside the submission limits`, () => {
    expect(ui.displayName.length).toBeLessThanOrEqual(30)
    expect(ui.displayName).not.toMatch(/\b(mcp|plugin|server)\b/i)
    expect(ui.shortDescription.length).toBeLessThanOrEqual(30)
    expect(ui.longDescription.length).toBeLessThanOrEqual(4000)
    expect(ui.developerName.length).toBeLessThanOrEqual(80)
    expect(ui.capabilities.length).toBeLessThanOrEqual(20)
    for (const c of ui.capabilities) expect(c.length).toBeLessThanOrEqual(120)
    expect(ui.defaultPrompt.length).toBeLessThanOrEqual(3)
    for (const p of ui.defaultPrompt) {
      expect(p.length).toBeLessThanOrEqual(128)
      expect(p, `starter prompts carry no @mention`).not.toContain(`@`)
    }
    expect(new Set(ui.defaultPrompt).size).toBe(ui.defaultPrompt.length)
    for (const url of [
      ui.websiteURL,
      ui.supportURL,
      ui.privacyPolicyURL,
      ui.termsOfServiceURL,
    ]) {
      expect(url).toMatch(/^https:\/\//)
      expect(url.length).toBeLessThanOrEqual(1024)
    }
    expect(ui.brandColor).toMatch(/^#[0-9A-F]{6}$/)
    expect(ui.brandColorDark).toMatch(/^#[0-9A-F]{6}$/)
  })

  it(`sells nothing and compares with nobody`, () => {
    for (const text of [
      manifest.description,
      ui.shortDescription,
      ui.longDescription,
      ...ui.capabilities,
      ...ui.defaultPrompt,
    ]) {
      expect(text, text).not.toMatch(FORBIDDEN_COPY)
    }
    expect(review.commerce).toBe(false)
  })

  it(`has no screenshots: the plugin has no UI`, () => {
    expect(ui.screenshots).toBeUndefined()
  })

  it(`ships every referenced asset, square`, () => {
    for (const rel of [ui.composerIcon, ui.composerIconDark, ui.logo]) {
      expect(rel).toMatch(/^\.\/assets\//)
      expect(existsSync(join(root, rel)), rel).toBe(true)
    }
    for (const rel of [ui.composerIcon, ui.composerIconDark]) {
      const svg = read(rel)
      const viewBox = svg.match(/viewBox="0 0 (\d+) (\d+)"/)
      expect(viewBox, `${rel} has a square viewBox`).not.toBeNull()
      expect(viewBox![1]).toBe(viewBox![2])
      expect(Number(viewBox![1])).toBeGreaterThanOrEqual(48)
    }
    // PNG IHDR: width and height are the two big-endian u32s at 16 and 20.
    const png = readFileSync(join(root, ui.logo))
    const width = png.readUInt32BE(16)
    const height = png.readUInt32BE(20)
    expect(width).toBe(height)
    expect(width).toBeGreaterThanOrEqual(48)
    expect(width).toBeLessThanOrEqual(4096)
    expect(png.byteLength).toBeLessThanOrEqual(5 * 1024 * 1024)
  })

  it(`points onboarding at a packaged skill`, () => {
    const rel = manifest.extensions[`com.openai`].onboardingSkill
    expect(rel).toMatch(/^\.\/skills\/[a-z0-9-]+\/SKILL\.md$/)
    expect(existsSync(join(root, rel))).toBe(true)
  })

  it(`carries five positive and three negative review cases naming real tools`, () => {
    expect(review.test_cases.positive).toHaveLength(5)
    expect(review.test_cases.negative).toHaveLength(3)
    for (const c of review.test_cases.positive) {
      for (const tool of c.tools_triggered.split(`,`).map((t) => t.trim())) {
        expect(knownTools.has(tool), `${c.description}: ${tool}`).toBe(true)
      }
      expect(c.expected_behavior.length).toBeGreaterThan(20)
    }
    expect(
      manifest.extensions[`com.openai`].publication.countries.length
    ).toBeGreaterThan(0)
    expect(
      manifest.extensions[`com.openai`].publication.release_notes.length
    ).toBeGreaterThan(20)
  })
})

describe(`mcp.json`, () => {
  it(`names the cloud MCP endpoint over streamable HTTP`, () => {
    const mcp = JSON.parse(read(`mcp.json`)) as {
      $schema: string
      mcpServers: Record<string, { type: string; url: string }>
    }
    expect(mcp.$schema).toBe(
      `https://agent-plugins.org/schemas/1.0.0/mcp.schema.json`
    )
    const servers = Object.values(mcp.mcpServers)
    // "Only one MCP server can be connected per plugin."
    expect(servers).toHaveLength(1)
    expect(servers[0]).toEqual({
      type: `streamable-http`,
      url: `https://app.exponential.at/api/mcp`,
    })
  })
})

describe(`skills`, () => {
  const skillDirs = readdirSync(join(root, `skills`))

  it(`every skill has a SKILL.md with a matching name and a description`, () => {
    expect(skillDirs.length).toBeGreaterThan(0)
    for (const dir of skillDirs) {
      const md = read(`skills/${dir}/SKILL.md`)
      const fm = md.match(/^---\n([\s\S]*?)\n---\n/)
      expect(fm, `${dir}: frontmatter`).not.toBeNull()
      expect(fm![1]).toMatch(new RegExp(`^name: ${dir}$`, `m`))
      expect(fm![1]).toMatch(/^description: >?/m)
    }
  })

  it(`stays provider-neutral and never sells`, () => {
    for (const dir of skillDirs) {
      const md = read(`skills/${dir}/SKILL.md`)
      // The submit-claude-plugin guide: "replace Claude-specific references
      // with provider-neutral language" — the model is "the model", and the
      // only agents named are the CLIs a coding session runs.
      expect(md, dir).not.toMatch(/\bClaude\b(?! Code)/)
      expect(md, dir).not.toMatch(FORBIDDEN_COPY)
      // Nothing that reads like a credential.
      expect(md, dir).not.toMatch(/expu_[A-Za-z0-9]{8,}/)
    }
  })

  it(`names only tools the server registers`, () => {
    for (const dir of skillDirs) {
      const md = read(`skills/${dir}/SKILL.md`)
      for (const m of md.matchAll(/`(exponential_[a-z_]+)`/g)) {
        expect(knownTools.has(m[1]), `${dir}: ${m[1]}`).toBe(true)
      }
    }
  })
})
