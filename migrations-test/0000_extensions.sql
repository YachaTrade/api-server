-- Enable pg_trgm extension required by GIN indexes in 0001_account.sql and 0002_token.sql.
CREATE EXTENSION IF NOT EXISTS pg_trgm;
