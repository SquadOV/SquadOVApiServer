ALTER TABLE new_wow_instances
DROP CONSTRAINT new_wow_instances_match_uuid_instance_id_instance_type_pla_excl;

ALTER TABLE new_wow_instances
ADD CONSTRAINT new_wow_instances_match_uuid_instance_id_instance_type_pla_excl EXCLUDE USING GIST (
    match_uuid WITH <>,
    instance_id WITH =,
    instance_type WITH =,
    players gist__intbig_ops WITH &&,
    tr WITH &&
);