resource "aws_glue_catalog_database" "glue_db" {
    name = "squadov-glue-database"
}

resource "aws_iam_role" "glue_role" {
    name = "glue-role"
    assume_role_policy = <<POLICY
{
    "Version": "2012-10-17",
    "Statement": [
        {
            "Effect": "Allow",
            "Principal": {
                "Service": [
                    "glue.amazonaws.com"
                ]
            },
            "Action": "sts:AssumeRole"
        }
    ]
}
POLICY
}

resource "aws_iam_policy" "glue_extra_policy" {
    name = "glue_extra_policy"
    description = "Extra policy for Glue."

    policy = <<EOF
{
    "Version": "2012-10-17",
    "Statement": [
        {
            "Effect": "Allow",
            "Action": [
                "kms:GenerateDataKey",
                "kms:Decrypt",
                "kms:Encrypt",
                "s3:ListBucket",
                "s3:GetObject",
                "s3:PutObject",
                "s3:DeleteObject",
                "secretsmanager:Get*"
            ],
            "Resource": "*"
        }
    ]
}
EOF
}

resource "aws_iam_role_policy_attachment" "glue_extra_policy_attach" {
    role = aws_iam_role.glue_role.name
    policy_arn = aws_iam_policy.glue_extra_policy.arn
}

resource "aws_iam_role_policy_attachment" "glue_glue_policy_attach" {
    policy_arn = "arn:aws:iam::aws:policy/service-role/AWSGlueServiceRole"
    role       = aws_iam_role.glue_role.name
}

resource "aws_glue_crawler" "rds_crawler" {
    database_name = aws_glue_catalog_database.glue_db.name
    name = "rds-crawler"
    role = aws_iam_role.glue_role.arn

    jdbc_target {
        connection_name = var.db_glue_connection_name
        path            = "squadov/squadov/%"
    }
}

resource "aws_glue_crawler" "redshift_crawler" {
    database_name = aws_glue_catalog_database.glue_db.name
    name = "redshift-crawler"
    role = aws_iam_role.glue_role.arn

    jdbc_target {
        connection_name = aws_glue_connection.redshift_connection.name
        path            = "squadov/%"
    }
}