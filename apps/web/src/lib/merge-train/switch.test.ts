import { afterEach, describe, expect, it } from "vitest"
import { mergeTrainBatchMax, mergeTrainOn, mergeTrainScoped } from "./worker"

// #EXP-1711: the dogfood switch — MERGE_TRAIN_REPOS scopes the train.
describe(`merge train switch`, () => {
  const saved = { on: process.env.MERGE_TRAIN, repos: process.env.MERGE_TRAIN_REPOS }
  afterEach(() => {
    if (saved.on === undefined) delete process.env.MERGE_TRAIN
    else process.env.MERGE_TRAIN = saved.on
    if (saved.repos === undefined) delete process.env.MERGE_TRAIN_REPOS
    else process.env.MERGE_TRAIN_REPOS = saved.repos
  })

  it(`off: never on, never scoped`, () => {
    delete process.env.MERGE_TRAIN
    process.env.MERGE_TRAIN_REPOS = `Niach/exponential`
    expect(mergeTrainOn(`Niach/exponential`)).toBe(false)
    expect(mergeTrainScoped()).toBe(false)
  })

  it(`on without a list: every repository`, () => {
    process.env.MERGE_TRAIN = `true`
    delete process.env.MERGE_TRAIN_REPOS
    expect(mergeTrainOn()).toBe(true)
    expect(mergeTrainOn(`other/repo`)).toBe(true)
    expect(mergeTrainScoped()).toBe(false)
  })

  it(`on with a list: only the named repositories, case-insensitive`, () => {
    process.env.MERGE_TRAIN = `true`
    process.env.MERGE_TRAIN_REPOS = ` niach/Exponential , a/b `
    expect(mergeTrainOn(`Niach/exponential`)).toBe(true)
    expect(mergeTrainOn(`a/b`)).toBe(true)
    expect(mergeTrainOn(`m5software/m5`)).toBe(false)
    expect(mergeTrainOn()).toBe(false)
    expect(mergeTrainScoped()).toBe(true)
  })
})

describe(`merge train batch cap`, () => {
  const saved = process.env.MERGE_TRAIN_BATCH_MAX
  afterEach(() => {
    if (saved === undefined) delete process.env.MERGE_TRAIN_BATCH_MAX
    else process.env.MERGE_TRAIN_BATCH_MAX = saved
  })
  it.each([
    [undefined, 8],
    [`4`, 4],
    [`1`, 1],
    [`20`, 8],
    [`0`, 8],
    [`x`, 8],
  ])(`MERGE_TRAIN_BATCH_MAX=%s → %i`, (value, expected) => {
    if (value === undefined) delete process.env.MERGE_TRAIN_BATCH_MAX
    else process.env.MERGE_TRAIN_BATCH_MAX = value
    expect(mergeTrainBatchMax()).toBe(expected)
  })
})
