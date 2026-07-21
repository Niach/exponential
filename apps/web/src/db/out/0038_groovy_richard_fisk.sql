CREATE TABLE "email_bounces" (
	"id" uuid PRIMARY KEY DEFAULT gen_random_uuid() NOT NULL,
	"email" varchar(320) NOT NULL,
	"kind" varchar(16) NOT NULL,
	"bounce_type" varchar(32),
	"bounce_sub_type" varchar(64),
	"diagnostic" text,
	"event_count" integer DEFAULT 1 NOT NULL,
	"last_event_at" timestamp with time zone NOT NULL,
	"suppressed_at" timestamp with time zone,
	"created_at" timestamp with time zone DEFAULT now() NOT NULL,
	"updated_at" timestamp with time zone DEFAULT now() NOT NULL,
	CONSTRAINT "email_bounces_email_unique" UNIQUE("email")
);
