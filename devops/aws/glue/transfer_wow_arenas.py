import sys
from awsglue.transforms import *
from awsglue.utils import getResolvedOptions
from pyspark.context import SparkContext
from pyspark.sql.session import SparkSession
from awsglue.context import GlueContext
from awsglue.job import Job
from awsglue.dynamicframe import DynamicFrame
from pyspark.sql.functions import collect_list, struct, lit, to_json, col, coalesce
import boto3
import json

args = getResolvedOptions(sys.argv, ['JOB_NAME', 'TempDir', 'IamRole', 'AwsRegion' , 'DbEndpoint', 'DbSecret'])
sc = SparkContext.getOrCreate()
spark = SparkSession(sc)
spark.conf.set("spark.sql.shuffle.partitions",128)

glueContext = GlueContext(sc)
print('JOB_NAME: ', args['JOB_NAME'])
print('TempDir: ', args['TempDir'])
print('IamRole: ', args['IamRole'])
print('AwsRegion: ', args['AwsRegion'])
print('DbEndpoint: ', args['DbEndpoint'])
print('DbSecret: ', args['DbSecret'])
job = Job(glueContext)
job.init(args['JOB_NAME'], args)

secretManager = boto3.client('secretsmanager', region_name=args['AwsRegion'])
dbSecretResp = secretManager.get_secret_value(SecretId=args['DbSecret'])
dbSecret = json.loads(dbSecretResp['SecretString'])

# This gets all the arena views since the last time this job was run.
print('Get WoW Match Views...')
wowMatchViews = glueContext.create_dynamic_frame.from_catalog(
    database='squadov-glue-database',
    table_name='squadov_squadov_wow_match_view',
    transformation_ctx='wowMatchViews',
    additional_options = {"jobBookmarkKeys":['start_tm'],'jobBookmarkKeysSortOrder':'asc'}
).toDF()

print('Get WoW Arena Views...')
wowArenaViews = glueContext.create_dynamic_frame.from_catalog(
    database='squadov-glue-database',
    table_name='squadov_squadov_wow_arena_view'
).toDF()

print('Join Arena Views...')
validArenaMatchViews = wowMatchViews.join(
    wowArenaViews,
    wowMatchViews['id'] == wowArenaViews['view_id'],
    'inner'
).dropDuplicates(
    ['match_uuid']
).filter(
    (wowMatchViews['advanced_log'] == True) & (wowMatchViews['end_tm'].isNotNull())
).drop(
    'advanced_log',
    'player_rating',
    'player_spec',
    't0_specs',
    't1_specs',
    'player_team',
    'session_id'
)

# Now we need to fenagle the data frame into the format that we ewant for storing into Redshift.
# Note that we're going to be storing the data into two separate tables. One general match table
# and one table for combatant information.

# For the match table, we need 'id', 'tm', 'build', and 'info'.
# So as for mapping goes:
#   - 'id' -> 'id'
#   - 'start_tm' -> 'tm'
#   - 'build_version' -> 'build'
#   - {'instance_id', 'arena_type', 'winning_team_id', 'match_duration_seconds'} -> 'info'
print('Transform Arena Matches to Output...')
outputMatchData = validArenaMatchViews.select(
    'id',
    'start_tm',
    'build_version',
    to_json(struct('instance_id', 'arena_type', 'winning_team_id', 'match_duration_seconds')).alias('info')
).withColumnRenamed(
    'build_version',
    'build'
).withColumnRenamed(
    'start_tm',
    'tm'
).withColumn(
    'match_type',
    lit('arena')
).na.drop()

# Write match data and combatant data to redshift.
print('Write Arena Match Data...', outputMatchData.count())
glueContext.write_dynamic_frame.from_catalog(
    frame=DynamicFrame.fromDF(outputMatchData, glueContext, 'outputMatchData'),
    database='squadov-glue-database', 
    table_name="squadov_public_wow_matches", 
    redshift_tmp_dir=args['TempDir'], 
    additional_options={'aws_iam_role': args['IamRole']})

# For the match combatant table, we want to insert a new row for each combatant in each match
print('Get WoW Match Characters...')
wowMatchCharacters = (spark
    .read
    .format('jdbc')
    .option('url', 'jdbc:postgresql://{}/squadov'.format(args['DbEndpoint']))
    .option('user', dbSecret['username'])
    .option('password', dbSecret['password'])
    .option('query', 'SELECT character_id, view_id, unit_guid FROM squadov.wow_match_view_character_presence WHERE has_combatant_info')
    .load()
)

# First, we want to get the valid characters with combatant infos.
print('Join characters to arena views...')
validMatchCharacters = validArenaMatchViews.join(
    wowMatchCharacters,
    validArenaMatchViews['id'] == wowMatchCharacters['view_id'],
    'inner'
).select(
    'id',
    'character_id',
    'unit_guid'
).repartition(128, 'character_id')

print('...Caching characters')
validMatchCharacters.cache()

print('vmc parts: ', validMatchCharacters.rdd.getNumPartitions())

# Next we need to make sure all the combatant info is transformed into the proper format (aka only having 1 row per combatant).
print('Get WoW Match Combatants...')
wowMatchCombatants = glueContext.create_dynamic_frame.from_catalog(
    database='squadov-glue-database',
    table_name='squadov_squadov_wow_match_view_combatants',
    transformation_ctx='wowMatchCombatants',
    additional_options = {"jobBookmarkKeys":['event_id'],'jobBookmarkKeysSortOrder':'asc'}
).toDF().select(
    'character_id',
    'team',
    'spec_id',
    'rating',
    'class_id'
).repartition(128, 'character_id')

print('wmc parts: ', wowMatchCombatants.rdd.getNumPartitions())

print('Join Combatant Info...')
joinedCombatantInfo = wowMatchCombatants.join(
    validMatchCharacters,
    wowMatchCombatants['character_id'] == validMatchCharacters['character_id'],
    'inner'
).drop(wowMatchCombatants['character_id'])

print('Get WoW Match Combatant Covenants...')
wowCombatantCovenants = glueContext.create_dynamic_frame.from_catalog(
    database='squadov-glue-database',
    table_name='squadov_squadov_wow_match_view_combatant_covenants'
).toDF().repartition(128, 'character_id')

print('wcc parts: ', wowCombatantCovenants.rdd.getNumPartitions())

print('Join Combatant Covenants...')
joinedCombatantCovenants = wowCombatantCovenants.join(
    validMatchCharacters,
    wowCombatantCovenants['character_id'] == validMatchCharacters['character_id'],
    'inner'
).drop(validMatchCharacters['character_id']).groupBy('character_id').agg(to_json(collect_list(struct('covenant_id', 'soulbind_id', 'soulbind_traits', 'conduit_item_ids', 'conduit_item_ilvls'))).alias('covenant'))

print('Get WoW Match Combatant Items...')
wowCombatantItems = glueContext.create_dynamic_frame.from_catalog(
    database='squadov-glue-database',
    table_name='squadov_squadov_wow_match_view_combatant_items'
).toDF().repartition(128, 'character_id')

print('wci parts: ', wowCombatantItems.rdd.getNumPartitions())

print('Join Combatant Items...')
joinedCombatantItems = wowCombatantItems.join(
    validMatchCharacters,
    wowCombatantItems['character_id'] == validMatchCharacters['character_id'],
    'inner'
).drop(validMatchCharacters['character_id']).groupBy('character_id').agg(to_json(collect_list(struct('idx', 'item_id', 'ilvl'))).alias('items'))

print('Get WoW Match Combatant Talents...')
wowCombatantTalents = glueContext.create_dynamic_frame.from_catalog(
    database='squadov-glue-database',
    table_name='squadov_squadov_wow_match_view_combatant_talents'
).toDF().repartition(128, 'character_id')

print('wct parts: ', wowCombatantTalents.rdd.getNumPartitions())

print('Join Combatant Talents...')
joinedCombatantTalents = wowCombatantTalents.join(
    validMatchCharacters,
    wowCombatantTalents['character_id'] == validMatchCharacters['character_id'],
    'inner'
).drop(validMatchCharacters['character_id']).groupBy('character_id').agg(to_json(collect_list(struct('talent_id', 'is_pvp'))).alias('talents'))

print('Join Combatant <> Covenant...')
st1 = joinedCombatantInfo.join(
    joinedCombatantCovenants,
    joinedCombatantInfo['character_id'] == joinedCombatantCovenants['character_id'],
    'left'
).drop(joinedCombatantCovenants['character_id'])

print('Join Combatant <> Items')
st2 = st1.join(
    joinedCombatantItems,
    st1['character_id'] == joinedCombatantItems['character_id'],
    'left'
).drop(joinedCombatantItems['character_id'])

print('Join Combatant <> Talents')
st3 = st2.join(
    joinedCombatantTalents,
    st2['character_id'] == joinedCombatantTalents['character_id'],
    'left'
).drop(joinedCombatantTalents['character_id'])

print('Select Combatant Columns for Output')
outputMatchCombatantData = st3.select(
    col('id').alias('match_id'),
    col('unit_guid').alias('player_guid'),
    'spec_id',
    coalesce(col('class_id'), lit(-1)).alias('class_id'),
    'rating',
    'team',
    'items',
    coalesce(col('talents'), lit('[]')).alias('talents'),
    coalesce(col('covenant'), lit('[]')).alias('covenant')
).na.drop(how='any', subset=['match_id', 'player_guid', 'spec_id', 'rating', 'team', 'items'])

outputMatchCombatantData.printSchema()

print('Write Combatant Match Data...')
glueContext.write_dynamic_frame.from_catalog(
    frame=DynamicFrame.fromDF(outputMatchCombatantData, glueContext, 'outputMatchCombatantData'),
    database='squadov-glue-database', 
    table_name="squadov_public_wow_match_combatants", 
    redshift_tmp_dir=args['TempDir'], 
    additional_options={'aws_iam_role': args['IamRole']})

print('Commit Job...')
job.commit()