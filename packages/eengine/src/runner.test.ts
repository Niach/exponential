// runner.json is the contract; the Rust twin (`eengine::runner`) replays the same cases.
import { describe, expect, test } from "bun:test"

import { supportedCatalogIds, validatePackage } from "@exponential_app/eui/host"

import fixture from "../fixtures/runner.json" with { type: "json" }
import { runnerConfig, runnerFailure, runnerPackage, runnerToast, type FormFactor, type RunnerFailure } from "./runner"

describe(`runner`, () => {
  for (const c of fixture.configCases)
    test(`config: ${c.name}`, () => {
      expect(runnerConfig(c.input)).toEqual(c.expected as never)
    })
  for (const c of fixture.shellCases) {
    test(`shell: ${c.name}`, () => {
      expect(runnerPackage(c.package, c.formFactor as FormFactor)).toEqual(c.expected)
    })
    test(`shell: ${c.name}: genUI accepts the wrapped package`, () => {
      expect(validatePackage(c.expected, supportedCatalogIds([]))).toEqual([])
    })
  }
  for (const c of fixture.failureCases)
    test(`failure: ${c.kind}`, () => {
      expect(runnerFailure(c.kind as RunnerFailure)).toEqual(c.expected)
    })
  for (const c of fixture.toastCases)
    test(`toast`, () => {
      expect(runnerToast(c.message)).toEqual(c.expected)
    })
  test(`every bundle location is named for the six targets`, () => {
    expect(Object.keys(fixture.bundle.locations).sort()).toEqual([`android`, `ios`, `linux`, `macos`, `web`, `windows`])
  })
})
