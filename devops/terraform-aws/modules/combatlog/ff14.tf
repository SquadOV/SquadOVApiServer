resource "aws_kinesis_stream" "ff14_stream" {
    name = "ff14-stream"
    retention_period = 24
    encryption_type = "KMS"
    kms_key_id = "alias/aws/kinesis"
    stream_mode_details {
        stream_mode = "ON_DEMAND"
    }
}

resource "aws_apigatewayv2_integration" "ff14_stream_integration" {
    api_id = aws_apigatewayv2_api.combat_log_gateway.id
    integration_type = "AWS_PROXY"
    integration_subtype = "Kinesis-PutRecord"
    payload_format_version = "1.0"
    credentials_arn = aws_iam_role.combat_log_role.arn

    request_parameters = {
        "StreamName" = "${aws_kinesis_stream.ff14_stream.name}"
        "Data" = "$request.body"
        "PartitionKey" = "$request.header.partition"
        "SequenceNumberForOrdering" = "$request.header.sequence"
        "Region" = "us-east-2"
    }
}

resource "aws_apigatewayv2_route" "ff14_stream_route" {
    api_id = aws_apigatewayv2_api.combat_log_gateway.id
    route_key = "PUT /ff14"
    authorization_type = "AWS_IAM"
    target = "integrations/${aws_apigatewayv2_integration.ff14_stream_integration.id}"
}

resource "aws_lambda_function" "ff14_combat_log_lambda" {
    function_name = "ff14-combat-log-lambda"
    role = aws_iam_role.lambda_role.arn

    filename = "../../aws/lambda/ff14.zip"
    source_code_hash = filebase64sha256("../../aws/lambda/ff14.zip")

    handler = "not.used"
    memory_size = 128
    package_type = "Zip"
    reserved_concurrent_executions = 64
    runtime = "provided.al2"
    timeout = 30

    tags = {
        "lambda" = "ff14"
    }

    environment {
        variables = {
            "SQUADOV_AWS_REGION" = "us-east-2"
            "SQUADOV_FIREHOSE_DELIVERY_STREAM" = aws_kinesis_firehose_delivery_stream.combat_log_s3_stream.name
        }
    }

    vpc_config {
        subnet_ids = var.lambda_subnets
        security_group_ids = var.lambda_security_groups
    }
}

resource "aws_lambda_event_source_mapping" "ff14_lambda_kinesis" {
    event_source_arn  = aws_kinesis_stream.ff14_stream.arn
    function_name     = aws_lambda_function.ff14_combat_log_lambda.arn
    starting_position = "LATEST"

    maximum_batching_window_in_seconds = 15
    maximum_record_age_in_seconds = -1
    maximum_retry_attempts = 0
    parallelization_factor = 8
}