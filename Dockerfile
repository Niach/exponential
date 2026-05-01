FROM oven/bun:1 AS builder
WORKDIR /app
COPY package.json bun.lock bunfig.toml ./
COPY apps/web/package.json apps/web/package.json
COPY apps/marketing/package.json apps/marketing/package.json
RUN bun install --frozen-lockfile
COPY . .
RUN bun --filter @exp/web build

FROM oven/bun:1
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
RUN bun install --frozen-lockfile
RUN touch apps/web/.env
EXPOSE 3000
CMD ["sh", "-c", "bun --filter @exp/web migrate && bun .output/server/index.mjs"]
