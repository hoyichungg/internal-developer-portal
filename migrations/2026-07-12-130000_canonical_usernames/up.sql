-- Username is an authentication identifier, not a case-sensitive display name.
-- Refuse to guess when historical rows collapse to one identifier. Operators
-- must rename the conflicting accounts explicitly before retrying migration.
DO $$
DECLARE
    collision_sample TEXT;
    -- Matches Rust `str::trim` / Unicode White_Space at identifier boundaries.
    trim_characters CONSTANT TEXT :=
        E' \t\n\v\f\r' ||
        U&'\0085\00A0\1680\2000\2001\2002\2003\2004\2005\2006\2007\2008\2009\200A\2028\2029\202F\205F\3000';
BEGIN
    SELECT string_agg(canonical_username, ', ' ORDER BY canonical_username)
      INTO collision_sample
      FROM (
          SELECT lower(btrim(username, trim_characters)) AS canonical_username
            FROM users
           GROUP BY lower(btrim(username, trim_characters))
          HAVING count(*) > 1
           ORDER BY lower(btrim(username, trim_characters))
           LIMIT 10
      ) collisions;

    IF collision_sample IS NOT NULL THEN
        RAISE EXCEPTION
            'cannot enforce canonical usernames; case/whitespace collisions exist: %',
            collision_sample
            USING HINT = 'Rename each conflicting user explicitly, then rerun the migration.';
    END IF;

    IF EXISTS (
        SELECT 1
          FROM users
         WHERE username <> btrim(username, trim_characters)
            OR btrim(username, trim_characters) = ''
    ) THEN
        RAISE EXCEPTION
            'cannot enforce canonical usernames; blank or surrounding-whitespace usernames exist'
            USING HINT = 'Rename invalid users explicitly, then rerun the migration.';
    END IF;
END
$$;

-- Preserve the original casing of historical usernames for display and audit
-- compatibility, while preventing Admin/admin from ever becoming two accounts.
CREATE UNIQUE INDEX users_username_lower_unique
    ON users (lower(username));

ALTER TABLE users
    ADD CONSTRAINT users_username_trimmed_nonempty_check
    CHECK (
        username = btrim(
            username,
            E' \t\n\v\f\r' ||
            U&'\0085\00A0\1680\2000\2001\2002\2003\2004\2005\2006\2007\2008\2009\200A\2028\2029\202F\205F\3000'
        )
        AND username <> ''
    );
