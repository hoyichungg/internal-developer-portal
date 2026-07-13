ALTER TABLE users
    DROP CONSTRAINT users_username_trimmed_nonempty_check;

DROP INDEX users_username_lower_unique;
