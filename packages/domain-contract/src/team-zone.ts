import fixture from "../fixtures/team-zone.json" with { type: "json" }

// VAPP-201 (fixtures/team-zone.json): the TEAM TIME ZONE every app host
// reads dates in. TS reference; desktop domain::team_zone, iOS ExpCore
// TeamZone.swift and Android domain/TeamZone.kt replay the same cases.

export const TEAM_ZONE_COPY = fixture.copy
export const TEAM_ZONE_TEST_IDS = fixture.testIds

/** The zone a team's `time_zone` names, when the tz database knows it; null
 *  = the device's zone (no team zone, blank, or an unknown name). */
export function resolveZone(teamZone: string | null | undefined): string | null {
  const zone = teamZone?.trim()
  if (!zone) return null
  try {
    new Intl.DateTimeFormat(`en-US`, { timeZone: zone })
    return zone
  } catch {
    return null
  }
}

/** A zone's offset from UTC at an instant, in minutes east. */
export function zoneOffset(zone: string, ms: number): number {
  const parts = new Intl.DateTimeFormat(`en-US`, {
    timeZone: zone,
    hourCycle: `h23`,
    year: `numeric`,
    month: `numeric`,
    day: `numeric`,
    hour: `numeric`,
    minute: `numeric`,
    second: `numeric`,
  }).formatToParts(new Date(ms))
  const get = (type: string) => Number(parts.find((p) => p.type === type)?.value ?? 0)
  const wall = Date.UTC(get(`year`), get(`month`) - 1, get(`day`), get(`hour`), get(`minute`), get(`second`))
  return Math.round((wall - Math.floor(ms / 1000) * 1000) / 60_000)
}

/** The eUI host's `zone`: the team zone's offset function, else the device's. */
export function hostZone(teamZone: string | null | undefined): (ms: number) => number {
  const zone = resolveZone(teamZone)
  return zone ? (ms) => zoneOffset(zone, ms) : (ms) => -new Date(ms).getTimezoneOffset()
}
