-- MANUAL PRODUCTION MIGRATION: Dev Post moderation audit + title/body split.
-- Before applying: take a verified backup and schedule a Dev Post maintenance
-- window. Do NOT run this script on a database where migrations 0040 and/or
-- 0041 have already been applied; this file is for applying both changes
-- together to an unmigrated production database.

BEGIN;

CREATE TABLE IF NOT EXISTS public.dev_post_moderation_log (
    id               UUID        PRIMARY KEY,
    post_id          BIGINT      NOT NULL CHECK (post_id > 0),
    token_id         VARCHAR(42) NOT NULL,
    admin_account_id VARCHAR(42) NOT NULL,
    action           VARCHAR(8)  NOT NULL
        CHECK (action IN ('DELETE', 'RESTORE')),
    changed          BOOLEAN     NOT NULL,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);

CREATE INDEX IF NOT EXISTS idx_dev_post_moderation_log_post_created
    ON public.dev_post_moderation_log (post_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_dev_post_moderation_log_admin_created
    ON public.dev_post_moderation_log (admin_account_id, created_at DESC);

-- Guard the schema change and backfill together: re-running this script after
-- a successful title split must not reinterpret the already-split body.
DO $$
DECLARE
    title_marker CONSTANT TEXT := 'nads-pump:0041_dev_post_title:backfill-complete:v1';
    title_comment TEXT;
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'dev_post'
          AND column_name = 'title'
    ) THEN
        ALTER TABLE public.dev_post
            ADD COLUMN title TEXT NOT NULL DEFAULT '';

        WITH legacy AS (
            SELECT id, body AS legacy_body, strpos(body, E'\n') AS first_lf
            FROM public.dev_post
        )
        UPDATE public.dev_post AS dp
        SET title = CASE
                WHEN legacy.first_lf = 0 THEN legacy.legacy_body
                ELSE left(
                    legacy.legacy_body,
                    legacy.first_lf - 1
                        - CASE
                            WHEN legacy.first_lf > 1
                             AND substr(legacy.legacy_body, legacy.first_lf - 1, 1) = E'\r'
                            THEN 1 ELSE 0
                          END
                )
            END,
            body = CASE
                WHEN legacy.first_lf = 0 THEN ''
                ELSE substr(legacy.legacy_body, legacy.first_lf + 1)
            END
        FROM legacy
        WHERE dp.id = legacy.id;

        COMMENT ON COLUMN public.dev_post.title IS 'nads-pump:0041_dev_post_title:backfill-complete:v1';
    ELSE
        SELECT col_description('public.dev_post'::regclass, attnum)
        INTO title_comment
        FROM pg_attribute
        WHERE attrelid = 'public.dev_post'::regclass
          AND attname = 'title'
          AND NOT attisdropped;

        IF title_comment IS DISTINCT FROM title_marker THEN
            RAISE EXCEPTION
                'dev_post.title exists without completion marker for migration 0041; refusing to guess whether backfill ran';
        END IF;
    END IF;
END
$$;

COMMIT;
