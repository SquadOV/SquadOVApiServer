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