-- =====================================================
-- HACKATHON RESTORE: Restore data from backup tables
-- =====================================================
-- Run this to restore backed-up data. Order matters due to FK constraints.

-- Step 1: Clear current data (if any)
DELETE FROM hackathon_team_member;
DELETE FROM hackathon_project;
DELETE FROM hackathon_team;
DELETE FROM hackathon;

-- Step 2: Restore in FK order
INSERT INTO hackathon SELECT * FROM _backup_hackathon;
INSERT INTO hackathon_team SELECT * FROM _backup_hackathon_team;
INSERT INTO hackathon_team_member SELECT * FROM _backup_hackathon_team_member;
INSERT INTO hackathon_project SELECT * FROM _backup_hackathon_project;

-- Step 3: Verify restore counts
SELECT 'hackathon' AS table_name, COUNT(*) AS restored_count FROM hackathon
UNION ALL
SELECT 'hackathon_team', COUNT(*) FROM hackathon_team
UNION ALL
SELECT 'hackathon_team_member', COUNT(*) FROM hackathon_team_member
UNION ALL
SELECT 'hackathon_project', COUNT(*) FROM hackathon_project;

-- Step 4: Drop backup tables (optional, run after confirming restore)
-- DROP TABLE IF EXISTS _backup_hackathon_project;
-- DROP TABLE IF EXISTS _backup_hackathon_team_member;
-- DROP TABLE IF EXISTS _backup_hackathon_team;
-- DROP TABLE IF EXISTS _backup_hackathon;
