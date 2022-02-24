import sys
from awsglue.transforms import *
from awsglue.utils import getResolvedOptions
from pyspark.context import SparkContext
from awsglue.context import GlueContext
from awsglue.job import Job
from awsglue.dynamicframe import DynamicFrame

args = getResolvedOptions(sys.argv, ['JOB_NAME'])
glueContext = GlueContext(SparkContext.getOrCreate())
job = Job(glueContext)
job.init(args['JOB_NAME'], args)

# This gets all the arena views since the last time this job was run.
from awsglue.dynamicframe import DynamicFrame
wowMatchViews = DynamicFrame.fromDF(Join.apply(
    glueContext.create_dynamic_frame.from_catalog(
        database='squadov-glue-database',
        table_name='squadov_squadov_wow_match_view',
        transformation_ctx='wowMatchViews',
        additional_options = {"jobBookmarkKeys":['start_tm'],'jobBookmarkKeysSortOrder':'asc'}
    ),
    glueContext.create_dynamic_frame.from_catalog(
        database='squadov-glue-database',
        table_name='squadov_squadov_wow_arena_view'
    ),
    'id',
    'view_id'
).toDF().dropDuplicates(['match_uuid']),
    glueContext,
    'wowMatchViews'
).filter(
    lambda x: x['advanced_log'] and x['end_tm'] is not None,
    transformation_ctx='wowMatchViews-filter'
).drop_fields([
    'advanced_log',
    'player_rating',
    'player_spec',
    't0_specs',
    't1_specs',
    'player_team',
    'session_id'
],
    transformation_ctx='wowMatchViews-drop'
)

# Now we need to fenagle the data frame into the format that we ewant for storing into Redshift.
# Note that we're going to be storing the data into two separate tables. One general match table
# and one table for combatant information.

# For the match table, we need 'id', 'tm', 'build', and 'info'.
# So as for mapping goes:
#   - 'id' -> 'id'
#   - 'start_tm' -> 'tm'
#   - 'build_version' -> 'build'
#   - {'instance_id', 'arena_type', 'winning_team_id', 'match_duration_seconds', 'new_ratings'} -> 'info'
def transformMatchData(rec):
    rec['info'] = {}
    rec['info']['instance_id'] = rec['instance_id']
    rec['info']['arena_type'] = rec['arena_type']
    rec['info']['winning_team_id'] = rec['winning_team_id']
    rec['info']['match_duration_seconds'] = rec['match_duration_seconds']
    rec['info']['new_ratings'] = rec['new_ratings']
    rec['match_type'] = 'arena'
    del rec['instance_id']
    del rec['arena_type']
    del rec['winning_team_id']
    del rec['match_duration_seconds']
    del rec['new_ratings']
    return rec
outputMatchData = Map.apply(
    wowMatchViews.select_fields([
        'id',
        'start_tm',
        'build_version',
        'instance_id',
        'arena_type',
        'winning_team_id',
        'match_duration_seconds',
        'new_ratings'
    ]).rename_field(
        'start_tm',
        'tm'
    ).rename_field(
        'build_version',
        'build'
    ),
    transformMatchData
)

# For the match combatant table, we want to insert a new row for each combatant in each match of
wowMatchCharacters = glueContext.create_dynamic_frame.from_catalog(
    database='squadov-glue-database',
    table_name='squadov_squadov_wow_match_view_character_presence')

# First, we want to get the valid characters with combatant infos (they will have the 'has_combatant_info' flag checked).
validMatchCharacters = Join.apply(
    wowMatchViews,
    wowMatchCharacters,
    'id',
    'view_id'
).filter(
    lambda x: x['has_combatant_info']
)

# Next we need to make sure all the combatant info is transformed into the proper format (aka only having 1 row per combatant).
from pyspark.sql.functions import collect_list, struct

vmcDf = validMatchCharacters.toDF()

wowCombatantInfo = Join.apply(
    validMatchCharacters,
    glueContext.create_dynamic_frame.from_catalog(
        database='squadov-glue-database',
        table_name='squadov_squadov_wow_match_view_combatants'),
    'character_id',
    'character_id'
).toDF()

wowCombatantCovenants = Join.apply(
    validMatchCharacters,
    glueContext.create_dynamic_frame.from_catalog(
        database='squadov-glue-database',
        table_name='squadov_squadov_wow_match_view_combatant_covenants'),
    'character_id',
    'character_id'
).toDF().groupBy('character_id').agg(collect_list(struct('covenant_id', 'soulbind_id', 'soulbind_traits', 'conduit_item_ids', 'conduit_item_ilvls')).alias('covenant'))

wowCombatantItems = Join.apply(
    validMatchCharacters,
    glueContext.create_dynamic_frame.from_catalog(
        database='squadov-glue-database',
        table_name='squadov_squadov_wow_match_view_combatant_items'),
    'character_id',
    'character_id'
).toDF().groupBy('character_id').agg(collect_list(struct('idx', 'item_id', 'ilvl')).alias('items'))

wowCombatantTalents = Join.apply(
    validMatchCharacters,
    glueContext.create_dynamic_frame.from_catalog(
        database='squadov-glue-database',
        table_name='squadov_squadov_wow_match_view_combatant_talents'),
    'character_id',
    'character_id'
).toDF().groupBy('character_id').agg(collect_list(struct('talent_id', 'is_pvp')).alias('talents'))

outputMatchCombatantData = DynamicFrame.fromDF(vmcDf.join(
   wowCombatantInfo,
   vmcDf['character_id'] == wowCombatantInfo['character_id'],
   'inner'
).join(
    wowCombatantCovenants,
    vmcDf['character_id'] == wowCombatantCovenants['character_id'],
    'left'
).join(
    wowCombatantItems,
    vmcDf['character_id'] == wowCombatantItems['character_id'],
    'left'
).join(
    wowCombatantTalents,
    vmcDf['character_id'] == wowCombatantTalents['character_id'],
    'left'
),
    glueContext,
    'outputMatchCombatantData'
).select_fields([
    'id',
    'unit_guid',
    'spec_id',
    'class_id',
    'rating',
    'team',
    'items',
    'talents',
    'covenant'
])

# Write match data and combatant data to redshift.

job.commit()