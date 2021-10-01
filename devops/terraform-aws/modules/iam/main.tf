resource "aws_iam_user" "api_user" {
    name = "squadov-api-${var.user}-${var.environment}"
}

resource "aws_iam_policy" "s3_policy" {
    name = "squadov-s3-policy-${var.user}-${var.environment}"
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