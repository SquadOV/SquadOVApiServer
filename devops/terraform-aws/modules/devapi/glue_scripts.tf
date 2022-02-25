resource "aws_s3_bucket" "glue_job_bucket" {
    bucket = "squadov-glue-job-bucket${var.bucket_suffix}"
}

resource "aws_s3_bucket_object" "transfer_wow_arenas_script" {
    bucket = aws_s3_bucket.glue_job_bucket.id
    key = "transfer_wow_arenas.py"
    source = "../../aws/glue/transfer_wow_arenas.py"
    etag = filemd5("../../aws/glue/transfer_wow_arenas.py")
}