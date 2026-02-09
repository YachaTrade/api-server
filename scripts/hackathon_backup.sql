-- =====================================================
-- HACKATHON BACKUP: Export all hackathon data to temp tables
-- =====================================================
-- Run this BEFORE dropping/clearing hackathon tables

-- Step 1: Drop old backup tables if they exist
DROP TABLE IF EXISTS _backup_hackathon_project;
DROP TABLE IF EXISTS _backup_hackathon_team_member;
DROP TABLE IF EXISTS _backup_hackathon_team;
DROP TABLE IF EXISTS _backup_hackathon;

-- Step 2: Create backup tables
CREATE TABLE _backup_hackathon AS SELECT * FROM hackathon;
CREATE TABLE _backup_hackathon_team AS SELECT * FROM hackathon_team;
CREATE TABLE _backup_hackathon_team_member AS SELECT * FROM hackathon_team_member;
CREATE TABLE _backup_hackathon_project AS SELECT * FROM hackathon_project;

-- Step 2: Verify backup counts
SELECT 'hackathon' AS table_name, COUNT(*) AS backup_count FROM _backup_hackathon
UNION ALL
SELECT 'hackathon_team', COUNT(*) FROM _backup_hackathon_team
UNION ALL
SELECT 'hackathon_team_member', COUNT(*) FROM _backup_hackathon_team_member
UNION ALL
SELECT 'hackathon_project', COUNT(*) FROM _backup_hackathon_project;
