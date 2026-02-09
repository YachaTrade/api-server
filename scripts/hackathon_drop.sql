-- =====================================================
-- HACKATHON DROP: Clear all hackathon data
-- =====================================================
-- Run AFTER backup. Order matters due to FK constraints.

DELETE FROM hackathon_team_member;
DELETE FROM hackathon_project;
DELETE FROM hackathon_team;
DELETE FROM hackathon;
