CREATE TABLE "live_activity_follows" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"session_id" uuid NOT NULL,
	"fcm_token_id" uuid NOT NULL,
	"relay_activity_id" text,
	"content" text NOT NULL,
	"sent_at" timestamp with time zone NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "live_activity_follows_session_id_fcm_token_id_unique" UNIQUE("session_id","fcm_token_id")
);
--> statement-breakpoint
ALTER TABLE "fcm_tokens" ADD COLUMN "live_activities" boolean DEFAULT false NOT NULL;--> statement-breakpoint
ALTER TABLE "live_activity_follows" ADD CONSTRAINT "live_activity_follows_session_id_sessions_id_fk" FOREIGN KEY ("session_id") REFERENCES "public"."sessions"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "live_activity_follows" ADD CONSTRAINT "live_activity_follows_fcm_token_id_fcm_tokens_id_fk" FOREIGN KEY ("fcm_token_id") REFERENCES "public"."fcm_tokens"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_live_activity_follows_token" ON "live_activity_follows" USING btree ("fcm_token_id");