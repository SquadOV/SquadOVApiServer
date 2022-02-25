resource "aws_s3_bucket" "glue_tmp_job_bucket" {
    bucket = "squadov-glue-tmp-job-bucket${var.bucket_suffix}"

    lifecycle_rule {
        enabled = true
        abort_incomplete_multipart_upload_days = 1

        expiration {
            days = 1
        }
    }
}

resource "aws_cloudwatch_log_group" "glue_jobs" {
    name = "squadov_glue_jobs"
    retention_in_days = 14
}

resource "aws_glue_job" "transfer_wow_arenas" {
    name = "squadov-etl-transfer-wow-arenas"
    role_arn = aws_iam_role.glue_role.arn

    command {
        name = "glueetl"
        script_location = "s3://${aws_s3_bucket.glue_job_bucket.id}/${aws_s3_bucket_object.transfer_wow_arenas_script.id}"
        python_version = 3
    }

    glue_version = "3.0"
    worker_type = "G.1X"
    number_of_workers = 4
    max_retries = 0

    default_arguments = {
        "--job-name" = "squadov_transfer_wow_arenas"
        "--job-bookmark-option": "job-bookmark-enable"
        "--TempDir": "s3://${aws_s3_bucket.glue_tmp_job_bucket.id}/tmp"
        "--IamRole": aws_iam_role.redshift_role.arn
        "--continuous-log-logGroup"          = aws_cloudwatch_log_group.glue_jobs.name
        "--continuous-log-logStreamPrefix"   = "transfer_wow_arenas"
        "--enable-continuous-cloudwatch-log" = "true"
        "--enable-continuous-log-filter"     = "true"
        "--enable-metrics"                   = ""
    }
}