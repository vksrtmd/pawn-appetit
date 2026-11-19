-- Check if the named indexes exist on the Games table
-- Lists the indexes created by create_indexes.sql; the always-present
-- games_identity_idx, games_date_expr_idx and SQLite's automatic indexes are not among them.
SELECT name FROM pragma_index_list('Games')
WHERE origin = 'c' AND name NOT IN ('games_identity_idx', 'games_date_expr_idx');
