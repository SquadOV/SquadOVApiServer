-- Getting rid of aimlab_player_unique as this seems to be unnecessary and can cause failed upload of the aimlab tasks
ALTER TABLE aimlab_tasks
DROP CONSTRAINT aimlab_player_unique