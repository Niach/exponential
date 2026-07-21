FROM oven/bun:1.3.10-alpine AS builder
WORKDIR /app
COPY package.json bun.lock bunfig.toml ./
COPY apps/web/package.json apps/web/package.json
COPY apps/marketing/package.json apps/marketing/package.json
COPY apps/video/package.json apps/video/package.json
COPY apps/push-relay/package.json apps/push-relay/package.json
COPY packages/db-schema/package.json packages/db-schema/package.json
COPY packages/design-tokens/package.json packages/design-tokens/package.json
COPY packages/domain-contract/package.json packages/domain-contract/package.json
COPY packages/electric-protocol/package.json packages/electric-protocol/package.json
COPY packages/steer-ticket/package.json packages/steer-ticket/package.json
COPY apps/steer-relay/package.json apps/steer-relay/package.json
COPY packages/tsconfig/package.json packages/tsconfig/package.json
COPY packages/widget/package.json packages/widget/package.json
RUN bun install --frozen-lockfile
COPY . .
# Widget first: it emits loader.js/widget.js into apps/web/public, which the
# web build then copies into .output/public.
RUN bun --filter @exp/widget build && bun --filter @exp/web build

FROM oven/bun:1.3.10-alpine
WORKDIR /app
COPY --from=builder /app/apps/web/.output .output
COPY --from=builder /app/apps/web/src/db apps/web/src/db
COPY --from=builder /app/apps/web/drizzle.config.ts apps/web/drizzle.config.ts
COPY --from=builder /app/apps/web/tsconfig.json apps/web/tsconfig.json
COPY --from=builder /app/apps/web/package.json apps/web/package.json
COPY --from=builder /app/package.json .
COPY --from=builder /app/bun.lock .
COPY --from=builder /app/bunfig.toml .
COPY --from=builder /app/apps/marketing/package.json apps/marketing/package.json
COPY --from=builder /app/apps/video/package.json apps/video/package.json
COPY --from=builder /app/apps/push-relay/package.json apps/push-relay/package.json
COPY --from=builder /app/apps/steer-relay/package.json apps/steer-relay/package.json
COPY --from=builder /app/packages packages
RUN bun install --frozen-lockfile
RUN touch apps/web/.env
# REV2-6: Bun caps simultaneous outbound fetch() at 256 per process. Every
# Electric shape long-poll is proxied through one fetch() held open ~20-60s
# (apps/web/src/lib/electric-proxy.ts), and each fully-synced client holds 14
# of them — the default saturates at ~18 clients and then stalls ALL other
# outbound fetches (GitHub App tokens, push relay, Creem, steer relay).
# 65336 is Bun's documented maximum; it is a cap, not a preallocation. See the
# header comment in apps/web/src/server-bun.ts and
# packages/electric-protocol/README.md ("Infra knobs").
ENV BUN_CONFIG_MAX_HTTP_REQUESTS=65336
EXPOSE 3000
# start-period covers the migrate step before the server begins listening.
HEALTHCHECK --interval=30s --timeout=5s --start-period=60s --retries=3 \
  CMD bun -e "fetch('http://localhost:3000/api/health').then(r=>process.exit(r.ok?0:1)).catch(()=>process.exit(1))"
CMD ["sh", "-c", "bun --filter @exp/web migrate && bun .output/server/index.mjs"]
