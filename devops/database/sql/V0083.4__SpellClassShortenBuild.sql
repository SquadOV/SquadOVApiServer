UPDATE wow_spell_to_class
SET build_id = SPLIT_PART(build_id, '.', 1)