FROM oven/bun:1 AS builder
WORKDIR /app
COPY package.json bun.lock bunfig.toml ./
COPY marketing/package.json marketing/package.json
RUN bun install --frozen-lockfile
COPY . .
RUN bun run build

FROM oven/bun:1
WORKDIR /app
COPY --from=builder /app/.output .output
COPY --from=builder /app/src/db src/db
COPY --from=builder /app/drizzle.config.ts .
COPY --from=builder /app/tsconfig.json .
COPY --from=builder /app/package.json .
COPY --from=builder /app/bun.lock .
COPY --from=builder /app/bunfig.toml .
COPY --from=builder /app/marketing/package.json marketing/package.json
RUN bun install --frozen-lockfile
RUN touch .env
EXPOSE 3000
CMD ["sh", "-c", "bunx drizzle-kit migrate && bun .output/server/index.mjs"]
