import { envInt, TokenBucketLimiter } from "@/lib/widget/rate-limit"

// #LOGS-2: the tunnel's bucket, per signed-in user (`u:<id>`) else per client
// IP (`ip:<addr>`): burst 200 envelopes, 3,600 an hour (one a second) —
// errors, the sampled transactions and the batched logs of a busy session fit
// comfortably; a crash loop or a flood does not. Both env-tunable.
let limiter: TokenBucketLimiter | null = null

export function getMonitoringLimiter(): TokenBucketLimiter {
  limiter ??= new TokenBucketLimiter({
    capacity: envInt(`MONITORING_RATE_LIMIT_BURST`, 200),
    refillPerHour: envInt(`MONITORING_RATE_LIMIT_PER_HOUR`, 3600),
  })
  return limiter
}
