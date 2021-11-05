/* Auto share */
CREATE OR REPLACE FUNCTION fn_trigger_auto_share()
    RETURNS trigger AS
$$
DECLARE
    vod_user_id BIGINT;
    conn_match_uuid UUID;
    conn_video_uuid UUID;
BEGIN
    IF NEW.match_uuid IS NULL THEN
        RETURN NEW;
    END IF;

    SELECT id INTO vod_user_id
    FROM squadov.users
    WHERE uuid = NEW.user_uuid;

    IF NEW.is_clip THEN
        conn_match_uuid := NULL;
        conn_video_uuid := NEW.video_uuid;
    ELSE
        conn_match_uuid := NEW.match_uuid;
        conn_video_uuid := NEW.video_uuid;
    END IF;

    INSERT INTO squadov.share_match_vod_connections (
        match_uuid,
        video_uuid,
        source_user_id,
        dest_user_id,
        dest_squad_id,
        can_share,
        can_clip,
        parent_connection_id,
        share_depth
    )
    SELECT
        conn_match_uuid,
        conn_video_uuid,
        vod_user_id,
        uas.dest_user_id,
        uas.dest_squad_id,
        uas.can_share,
        uas.can_clip,
        NULL,
        0
    FROM squadov.user_autosharing_settings AS uas
    CROSS JOIN (
        SELECT *
        FROM squadov.matches AS m
        WHERE m.uuid = NEW.match_uuid
    ) AS m
    CROSS JOIN (
        SELECT *
        FROM squadov.vods AS v
        WHERE v.video_uuid = conn_video_uuid
    ) AS v
    LEFT JOIN squadov.user_autosharing_settings_games AS asg
        ON asg.id = uas.id
            AND asg.game = m.game
    WHERE uas.source_user_id = vod_user_id
        AND (uas.dest_user_id IS NOT NULL OR uas.dest_squad_id IS NOT NULL)
        AND asg.id IS NOT NULL
        AND NOT v.is_local;
    
    INSERT INTO squadov.user_profile_vods (
        user_id,
        video_uuid
    )
    SELECT
        vod_user_id,
        conn_video_uuid
    FROM squadov.user_autosharing_settings AS uas
    CROSS JOIN (
        SELECT *
        FROM squadov.matches AS m
        WHERE m.uuid = NEW.match_uuid
    ) AS m
    CROSS JOIN (
        SELECT *
        FROM squadov.vods AS v
        WHERE v.video_uuid = conn_video_uuid
    ) AS v
    LEFT JOIN squadov.user_autosharing_settings_games AS asg
        ON asg.id = uas.id
            AND asg.game = m.game
    WHERE uas.source_user_id = vod_user_id
        AND uas.dest_user_id IS NULL
        AND uas.dest_squad_id IS NULL
        AND asg.id IS NOT NULL
        AND NOT v.is_local
    ON CONFLICT DO NOTHING;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;