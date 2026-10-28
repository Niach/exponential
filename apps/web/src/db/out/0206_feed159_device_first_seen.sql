CREATE TABLE "device_first_seen" (
	"device_id" varchar(128) PRIMARY KEY NOT NULL,
	"user_id" text NOT NULL,
	"first_seen_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
ALTER TABLE "device_first_seen" ADD CONSTRAINT "device_first_seen_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
-- FEED-159 backfill: a pinned id keeps its pin holder (no existing pin
-- changes hands); every other id takes its eldest live row's account.
INSERT INTO "device_first_seen" ("device_id", "user_id", "first_seen_at")
SELECT "device_id", "user_id", "created_at" FROM "device_keys"
ON CONFLICT DO NOTHING;--> statement-breakpoint
INSERT INTO "device_first_seen" ("device_id", "user_id", "first_seen_at")
SELECT DISTINCT ON ("device_id") "device_id", "user_id", "created_at" FROM "devices"
ORDER BY "device_id", "created_at", "id"
ON CONFLICT DO NOTHING;
