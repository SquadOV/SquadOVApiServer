resource "aws_iam_user" "api_user" {
    name = "squadov-api-user"
}

resource "aws_iam_policy" "s3_policy" {
    name = "squadov-s3-policy"
    description = "Policy to allow API server to do required tasks in S3."

    policy = <<EOF
{
    "Version": "2012-10-17",
    "Statement": [
        {
            "Sid": "VisualEditor0",
            "Effect": "Allow",
            "Action": [
                "s3:PutObject",
                "s3:GetObjectAcl",
                "s3:GetObject",
                "s3:AbortMultipartUpload",
                "s3:DeleteObject",
                "s3:PutObjectAcl",
                "s3:PutObjectTagging",
                "s3:GetObjectTagging"
            ],
            "Resource": "*"
        }
    ]
}
EOF
}

resource "aws_iam_user_policy_attachment" "api_s3_policy_attach" {
    user = aws_iam_user.api_user.name
    policy_arn = aws_iam_policy.s3_policy.arn
}

resource "aws_cognito_identity_pool" "squadov_aws_identity_pool" {
    identity_pool_name = "squadov_aws_identity_pool${var.resource_suffix}"
    allow_unauthenticated_identities = false
    allow_classic_flow = false
    developer_provider_name = "squadovgg${var.resource_suffix}"
}

resource "aws_iam_role" "authenticated_cognito_role" {
    name = "cognito_authenticated"
    assume_role_policy = <<POLICY
{
    "Version": "2012-10-17",
    "Statement": [
        {
            "Effect": "Allow",
            "Principal": {
                "Federated": "cognito-identity.amazonaws.com"
            },
            "Action": "sts:AssumeRoleWithWebIdentity",
            "Condition": {
                "StringEquals": {
                    "cognito-identity.amazonaws.com:aud": "${aws_cognito_identity_pool.squadov_aws_identity_pool.id}"
                },
                "ForAnyValue:StringLike": {
                    "cognito-identity.amazonaws.com:amr": "authenticated"
                }
            }
        }
    ]
}
POLICY
}

resource "aws_iam_role_policy" "authenticated_cognito_policy" {
    name = "authenticated_cognito_policy"
    role = aws_iam_role.authenticated_cognito_role.id

    policy = <<EOF
{
    "Version": "2012-10-17",
    "Statement": [
        {
            "Effect": "Allow",
            "Action": [
                "mobileanalytics:PutEvents",
                "cognito-sync:*",
                "cognito-identity:*"
            ],
            "Resource": [
                "*"
            ]
        },
        {
            "Effect": "Allow",
            "Action": [
                "execute-api:Invoke"
            ],
            "Resource": [
                "arn:aws:execute-api:*:*:${var.api_gateway_id}/*/PUT/ff14"
            ]
        }
    ]
}
EOF
}

resource "aws_cognito_identity_pool_roles_attachment" "main" {
    identity_pool_id = aws_cognito_identity_pool.squadov_aws_identity_pool.id
    roles = {
        "authenticated" = aws_iam_role.authenticated_cognito_role.arn
    }
}