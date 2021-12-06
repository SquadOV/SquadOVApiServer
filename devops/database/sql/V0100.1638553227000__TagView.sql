CREATE OR REPLACE VIEW view_vod_tags AS
SELECT
    uvt.video_uuid,
    uvt.user_id,
    uvt.tm,
    t.tag
FROM user_vod_tags AS uvt
INNER JOIN tags AS t
    ON t.tag_id = uvt.tag_id;