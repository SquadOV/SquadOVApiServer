resource "aws_apigatewayv2_api" "combat_log_gateway" {
    name          = "combatlog-http-api"
    protocol_type = "HTTP"
}

resource "aws_iam_role" "combat_log_role" {
    name = "combat_log_role"
    assume_role_policy = <<POLICY
{
    "Version": "2012-10-17",
    "Statement": [
        {
            "Effect": "Allow",
            "Principal": {
                "Service": [
                    "apigateway.amazonaws.com"
                ]
            },
            "Action": "sts:AssumeRole"
        }
    ]
}
POLICY
}

resource "aws_iam_role_policy" "combat_log_policy" {
    name = "authenticated_cognito_policy"
    role = aws_iam_role.combat_log_role.id

    policy = <<EOF
{
    "Version": "2012-10-17",
    "Statement": [
        {
            "Effect": "Allow",
            "Action": [
                "kinesis:Put*"
            ],
            "Resource": [
                "*"
            ]
        }
    ]
}
EOF
}

resource "aws_apigatewayv2_stage" "combat_log_gateway_stage" {
    api_id = aws_apigatewayv2_api.combat_log_gateway.id
    auto_deploy = true
    name   = "$default"
}

output "api_gateway_id" {
    value = aws_apigatewayv2_api.combat_log_gateway.id
}

resource "aws_iam_role" "combatlog_firehose_role" {
  name = "combatlog-firehose-role"

  assume_role_policy = <<EOF
{
    "Version": "2012-10-17",
    "Statement": [
        {
            "Action": "sts:AssumeRole",
            "Principal": {
                "Service": "firehose.amazonaws.com"
            },
            "Effect": "Allow",
            "Sid": ""
        }
    ]
}
EOF
}

resource "aws_iam_policy" "combatlog_firehose_policy" {
    name = "combatlog-firehose-policy"
    description = "Policy to allow Firehose to write to S3."

    policy = <<EOF
{
    "Version": "2012-10-17",  
    "Statement":
    [    
        {      
            "Effect": "Allow",      
            "Action": [
                "s3:AbortMultipartUpload",
                "s3:GetBucketLocation",
                "s3:GetObject",
                "s3:ListBucket",
                "s3:ListBucketMultipartUploads",
                "s3:PutObject"
            ],      
            "Resource": [        
                "${var.combatlog_bucket_arn}",
                "${var.combatlog_bucket_arn}/*"		    
            ]
        }
    ]
}
EOF
}

resource "aws_iam_role_policy_attachment" "combatlog_firehose_attachment" {
    role = aws_iam_role.combatlog_firehose_role.name
    policy_arn = aws_iam_policy.combatlog_firehose_policy.arn
}

resource "aws_kinesis_firehose_delivery_stream" "combat_log_s3_stream" {
    name = "combatlog-firehose-to-s3-stream"
    destination = "extended_s3"

    server_side_encryption {
        enabled = true
    }

    extended_s3_configuration {
        role_arn = aws_iam_role.combatlog_firehose_role.arn
        bucket_arn = var.combatlog_bucket_arn

        buffer_size = 64
        buffer_interval = 60
        compression_format = "GZIP"

        prefix = "form=!{partitionKeyFromQuery:form}/partition=!{partitionKeyFromQuery:partition}/"
        error_output_prefix = "errors/year=!{timestamp:yyyy}/month=!{timestamp:MM}/day=!{timestamp:dd}/!{firehose:error-output-type}"

        processing_configuration {
            enabled = true

            processors {
                type = "MetadataExtraction"

                parameters {
                    parameter_name  = "MetadataExtractionQuery"
                    parameter_value = "{partition:.partition_id,form:.data.form}"
                }

                parameters {
                    parameter_name  = "JsonParsingEngine"
                    parameter_value = "JQ-1.6"
                }
            }
        }
        
        dynamic_partitioning_configuration {
            enabled = true
        }
    }
}

resource "aws_s3_bucket_notification" "combat_log_bucket_notification" {
    bucket = var.combatlog_bucket_id

    lambda_function {
        lambda_function_arn = aws_lambda_function.combat_log_reports_lambda.arn
        events = ["s3:ObjectCreated:*"]
        filter_prefix = "form%3DFlush/"
    }

    depends_on = [
        aws_lambda_permission.lambda_combatlog_bucket_permissions
    ]
}