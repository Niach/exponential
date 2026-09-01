CREATE TABLE "session_attachments" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"team_id" uuid NOT NULL,
	"session_id" uuid,
	"uploader_id" text,
	"filename" varchar(500) NOT NULL,
	"content_type" varchar(255) NOT NULL,
	"size_bytes" bigint NOT NULL,
	"storage_key" text NOT NULL,
	"url" text NOT NULL,
	"width" integer,
	"height" integer,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
ALTER TABLE "session_attachments" ADD CONSTRAINT "session_attachments_team_id_teams_id_fk" FOREIGN KEY ("team_id") REFERENCES "public"."teams"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "session_attachments" ADD CONSTRAINT "session_attachments_session_id_coding_sessions_id_fk" FOREIGN KEY ("session_id") REFERENCES "public"."coding_sessions"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "session_attachments" ADD CONSTRAINT "session_attachments_uploader_id_users_id_fk" FOREIGN KEY ("uploader_id") REFERENCES "public"."users"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_session_attachments_team" ON "session_attachments" USING btree ("team_id");--> statement-breakpoint
CREATE INDEX "idx_session_attachments_session" ON "session_attachments" USING btree ("session_id");