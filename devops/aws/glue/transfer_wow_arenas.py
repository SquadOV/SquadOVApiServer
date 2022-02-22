import sys
from awsglue.transforms import *
from awsglue.utils import getResolvedOptions
from pyspark.context import SparkContext
from awsglue.context import GlueContext
from awsglue.job import Job

glueContext = GlueContext(SparkContext.getOrCreate())

wowArenaViews = glueContext.create_dynamic_frame.from_catalog(
             database='squadov-glue-database',
             table_name='squadov_squadov_wow_arena_view')

wowMatchViews = glueContext.create_dynamic_frame.from_catalog(
             database='squadov-glue-database',
             table_name='squadov_squadov_wow_match_view')

wowMatchViews = glueContext.create_dynamic_frame.from_catalog(
             database='squadov-glue-database',
             table_name='squadov_squadov_wow_match_view')

print('Count: ', wowArenaViews.count())
wowArenaViews.printSchema()