import { afterEach, describe, expect, it } from "vitest"
import { mergeTrainBatchMax } from "./worker"

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
