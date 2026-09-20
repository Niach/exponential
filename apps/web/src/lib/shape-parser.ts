// The column parsers every Electric collection shares (lib/collections.ts).
//
// The Electric client hands each value over as the Postgres TEXT form and
// parses by type. Its defaults cover the timestamps but turn every `int8`
// into a JavaScript BigInt — and `attachments.size_bytes` IS an int8 (drizzle
// `bigint({ mode: "number" })`), so a 22 KB file synced as `22345n`, failed
// `Number.isFinite` in `formatAttachmentSize` and showed "0 B" (EXP-955). The
// schema's number mode is the contract every reader is written against:
// parse int8 to a number here, once, for all collections. Every synced int8
// is a byte count or an id far below 2^53.
export const shapeParser = {
  timestamp: (date: string) => new Date(date),
  timestamptz: (date: string) => new Date(date),
  int8: (value: string) => Number(value),
}
