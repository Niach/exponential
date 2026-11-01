CREATE TABLE "steer_connection_events" (
	"id" varchar(64) PRIMARY KEY NOT NULL,
	"at" timestamp with time zone NOT NULL,
	"received_at" timestamp with time zone DEFAULT now() NOT NULL,
	"kind" varchar(32) NOT NULL,
	"phase" varchar(16) NOT NULL,
	"platform" varchar(16) NOT NULL,
	"version" varchar(64) NOT NULL,
	"outcome" varchar(16) NOT NULL,
	"path" varchar(32),
	"first_frame_ms" integer,
	"error_class" varchar(32),
	"fallbacks" integer,
	"reconnects" integer,
	"duration_ms" integer,
	"failures" jsonb
);
--> statement-breakpoint
CREATE INDEX "idx_steer_connection_events_received" ON "steer_connection_events" USING btree ("received_at");