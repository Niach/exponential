CREATE TABLE "session_reads" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"user_id" text NOT NULL,
	"session_id" uuid NOT NULL,
	"read_at" timestamp with time zone DEFAULT now() NOT NULL,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
ALTER TABLE "messages" ADD COLUMN "author_session_id" uuid;--> statement-breakpoint
ALTER TABLE "sessions" ADD COLUMN "caption" varchar(80);--> statement-breakpoint
ALTER TABLE "sessions" ADD COLUMN "aside" boolean DEFAULT false NOT NULL;--> statement-breakpoint
ALTER TABLE "session_reads" ADD CONSTRAINT "session_reads_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "session_reads" ADD CONSTRAINT "session_reads_session_id_sessions_id_fk" FOREIGN KEY ("session_id") REFERENCES "public"."sessions"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
CREATE UNIQUE INDEX "uniq_session_reads_user_session" ON "session_reads" USING btree ("user_id","session_id");--> statement-breakpoint
CREATE INDEX "idx_session_reads_session" ON "session_reads" USING btree ("session_id");--> statement-breakpoint
ALTER TABLE "messages" ADD CONSTRAINT "messages_author_session_id_sessions_id_fk" FOREIGN KEY ("author_session_id") REFERENCES "public"."sessions"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_messages_author_session" ON "messages" USING btree ("author_session_id");--> statement-breakpoint
-- EXP-1323: existing sessions start READ for their starter (no bold backlog).
INSERT INTO "session_reads" ("user_id", "session_id", "read_at") SELECT "user_id", "id", now() FROM "sessions" ON CONFLICT DO NOTHING;