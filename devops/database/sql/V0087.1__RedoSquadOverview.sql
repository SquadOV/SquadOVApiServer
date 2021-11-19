DROP VIEW squad_overview;

CREATE OR REPLACE VIEW squad_overview AS
SELECT
    sq.*,
    smc.member_count,
    spic.pending_invite_count
FROM squads AS sq
INNER JOIN squad_member_count AS smc
    ON smc.id = sq.id
INNER JOIN squad_pending_invite_count AS spic
    ON spic.id = sq.id;