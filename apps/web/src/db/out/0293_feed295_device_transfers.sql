CREATE TABLE "device_transfers" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"user_id" text NOT NULL,
	"session_id" uuid,
	"from_device_id" varchar(128) NOT NULL,
	"to_device_id" varchar(128) NOT NULL,
	"src_path" text NOT NULL,
	"dest_path" text NOT NULL,
	"status" varchar(16) DEFAULT 'queued' NOT NULL,
	"files" integer,
	"files_done" integer DEFAULT 0 NOT NULL,
	"bytes" bigint,
	"bytes_done" bigint DEFAULT 0 NOT NULL,
	"skipped" jsonb,
	"error" text,
	"started_at" timestamp with time zone,
	"completed_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL
);
--> statement-breakpoint
ALTER TABLE "device_transfers" ADD CONSTRAINT "device_transfers_user_id_users_id_fk" FOREIGN KEY ("user_id") REFERENCES "public"."users"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "device_transfers" ADD CONSTRAINT "device_transfers_session_id_sessions_id_fk" FOREIGN KEY ("session_id") REFERENCES "public"."sessions"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "idx_device_transfers_user" ON "device_transfers" USING btree ("user_id","created_at");