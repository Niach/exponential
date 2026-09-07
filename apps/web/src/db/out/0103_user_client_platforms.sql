CREATE TABLE "user_client_platforms" (
	"user_id" text NOT NULL,
	"platform" varchar(16) NOT NULL,
	"first_seen_at" timestamp with time zone DEFAULT now() NOT NULL,
	"last_seen_at" timestamp with time zone DEFAULT now() NOT NULL,
	"last_version" varchar(32),
	CONSTRAINT "user_client_platforms_user_id_platform_pk" PRIMARY KEY("user_id","platform")
);
--> statement-breakpoint
ALTER TABLE "user_client_platforms" ADD CONSTRAINT "user_client_platforms_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_user_client_platforms_platform" ON "user_client_platforms" USING btree ("platform");--> statement-breakpoint
-- EXP-759 backfill from what already exists, APPROXIMATE by construction:
-- sessions expire after 60d; native Google/Apple and CLI device-code sessions
-- are minted by a browser (they look like web); desktop and the CLI (reqwest)
-- send no User-Agent at all — those come from `devices` below. Idempotent:
-- re-running keeps the earliest first_seen_at and the latest last_seen_at.
INSERT INTO "user_client_platforms" ("user_id", "platform", "first_seen_at", "last_seen_at")
SELECT "user_id",
       CASE WHEN "user_agent" ILIKE '%CFNetwork%' THEN 'ios'
            WHEN "user_agent" ILIKE '%okhttp%' OR "user_agent" ILIKE '%Dalvik%' THEN 'android'
            ELSE 'web' END,
       min("created_at"), max("updated_at")
FROM "sessions"
WHERE "user_agent" ILIKE '%CFNetwork%' OR "user_agent" ILIKE '%okhttp%'
   OR "user_agent" ILIKE '%Dalvik%' OR "user_agent" ILIKE 'Mozilla%'
GROUP BY 1, 2
ON CONFLICT ("user_id", "platform") DO UPDATE SET
  "first_seen_at" = least("user_client_platforms"."first_seen_at", excluded."first_seen_at"),
  "last_seen_at" = greatest("user_client_platforms"."last_seen_at", excluded."last_seen_at");--> statement-breakpoint
INSERT INTO "user_client_platforms" ("user_id", "platform", "first_seen_at", "last_seen_at", "last_version")
SELECT "user_id", CASE "kind" WHEN 'server' THEN 'cli' ELSE 'desktop' END,
       min("created_at"), max("last_seen_at"),
       (array_agg("version" ORDER BY "last_seen_at" DESC))[1]
FROM "devices" WHERE "kind" IN ('desktop', 'server')
GROUP BY 1, 2
ON CONFLICT ("user_id", "platform") DO UPDATE SET
  "first_seen_at" = least("user_client_platforms"."first_seen_at", excluded."first_seen_at"),
  "last_seen_at" = greatest("user_client_platforms"."last_seen_at", excluded."last_seen_at"),
  "last_version" = coalesce(excluded."last_version", "user_client_platforms"."last_version");--> statement-breakpoint
INSERT INTO "user_client_platforms" ("user_id", "platform", "first_seen_at", "last_seen_at")
SELECT "user_id", "platform", min("created_at"), max("updated_at")
FROM "fcm_tokens" WHERE "platform" IN ('ios', 'android')
GROUP BY 1, 2
ON CONFLICT ("user_id", "platform") DO UPDATE SET
  "first_seen_at" = least("user_client_platforms"."first_seen_at", excluded."first_seen_at"),
  "last_seen_at" = greatest("user_client_platforms"."last_seen_at", excluded."last_seen_at");
