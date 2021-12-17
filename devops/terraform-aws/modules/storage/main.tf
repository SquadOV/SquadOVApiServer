resource "aws_cloudfront_origin_access_identity" "private_vod_access_identity" {
    comment = "For Signed URL access to VODs"
}

resource "aws_cloudfront_origin_access_identity" "private_blob_access_identity" {
    comment = "For Signed URL access to Blobs"
}

resource "aws_cloudfront_origin_access_identity" "public_vod_access_identity" {    
    comment = "For public access to VODs"
}

resource "aws_s3_bucket" "vod_storage_bucket" {
    bucket = "squadov-us-vod-bucket${var.bucket_suffix}"
    acl = "private"

    lifecycle_rule {
        enabled = true
        abort_incomplete_multipart_upload_days = 1
        transition {
            days = 30
            storage_class = "STANDARD_IA"
        }

        transition {
            days = 90
            storage_class = "GLACIER_IR"
        }
    }

    server_side_encryption_configuration {
        rule {
            apply_server_side_encryption_by_default {
                sse_algorithm = "AES256"
            }
        }
    }

    tags = {
        "s3" = "vods"
        "vods" = "raw_s3"
    }
}

data "aws_iam_policy_document" "vod_access_policy" {
    statement {
        actions = ["s3:GetObject"]
        effect = "Allow"

        condition {
            test = "StringEquals"
            values = ["public"]
            variable = "s3:ExistingObjectTag/access"
        }

        principals {
            type = "AWS"
            identifiers = [aws_cloudfront_origin_access_identity.public_vod_access_identity.iam_arn]
        }

        resources = [ "${aws_s3_bucket.vod_storage_bucket.arn}/*"]

        sid = "cloudfront-public-access"
    }

    statement {
        actions = ["s3:GetObject"]
        effect = "Allow"

        principals {
            type = "AWS"
            identifiers = [aws_cloudfront_origin_access_identity.private_vod_access_identity.iam_arn]
        }

        resources = [ "${aws_s3_bucket.vod_storage_bucket.arn}/*"]

        sid = "cloudfront-private-access"
    }

    statement {
        actions = ["s3:*"]
        effect = "Deny"

        principals {
            type = "*"
            identifiers = ["*"]
        }

        condition {
            test = "Bool"
            values = ["false"]
            variable = "aws:SecureTransport"
        }

        resources = [ aws_s3_bucket.vod_storage_bucket.arn, "${aws_s3_bucket.vod_storage_bucket.arn}/*"]

        sid = "enforce-ssl"
    }
}

resource "aws_s3_bucket_policy" "vod_bucket_policy" {
    bucket = aws_s3_bucket.vod_storage_bucket.id
    policy = data.aws_iam_policy_document.vod_access_policy.json
}

resource "aws_s3_bucket" "speed_check_storage_bucket" {
    bucket = "squadov-us-speed-check-bucket${var.bucket_suffix}"
    acl = "private"

    lifecycle_rule {
        enabled = true
        abort_incomplete_multipart_upload_days = 1
    }

    server_side_encryption_configuration {
        rule {
            apply_server_side_encryption_by_default {
                sse_algorithm = "AES256"
            }
        }
    }

    tags = {
        "s3" = "speed_check"
        "speed_check" = "raw_s3"
    }
}

resource "aws_s3_bucket" "blob_storage_bucket" {
    bucket = "squadov-us-blob-bucket${var.bucket_suffix}"
    acl = "private"

    lifecycle_rule {
        enabled = true
        abort_incomplete_multipart_upload_days = 1
        transition {
            days = 30
            storage_class = "STANDARD_IA"
        }
    }

    server_side_encryption_configuration {
        rule {
            apply_server_side_encryption_by_default {
                sse_algorithm = "AES256"
            }
        }
    }

    tags = {
        "s3" = "blobs"
    }
}

data "aws_iam_policy_document" "blob_access_policy" {
    statement {
        actions = ["s3:GetObject"]
        effect = "Allow"

        principals {
            type = "AWS"
            identifiers = [aws_cloudfront_origin_access_identity.private_blob_access_identity.iam_arn]
        }

        resources = [ "${aws_s3_bucket.blob_storage_bucket.arn}/*"]

        sid = "cloudfront-private-access"
    }

    statement {
        actions = ["s3:*"]
        effect = "Deny"

        principals {
            type = "*"
            identifiers = ["*"]
        }

        condition {
            test = "Bool"
            values = ["false"]
            variable = "aws:SecureTransport"
        }

        resources = [ aws_s3_bucket.blob_storage_bucket.arn, "${aws_s3_bucket.blob_storage_bucket.arn}/*"]

        sid = "enforce-ssl"
    }
}

resource "aws_s3_bucket_policy" "blob_bucket_policy" {
    bucket = aws_s3_bucket.blob_storage_bucket.id
    policy = data.aws_iam_policy_document.blob_access_policy.json
}

resource "aws_cloudfront_public_key" "private_s3_vod_cloudfront_public_key" {
    name = "private-s3-vod-cloudfront-public-key${var.bucket_suffix}"
    encoded_key = file("../../aws/keys/private_s3_vod_cloudfront_PUBLIC_KEY.pem")
}

resource "aws_cloudfront_key_group" "private_s3_vod_cloudfront_key_group" {
    name = "private-s3-vod-cloudfront-key-group${var.bucket_suffix}"
    items = [ aws_cloudfront_public_key.private_s3_vod_cloudfront_public_key.id ]
}

resource "aws_cloudfront_distribution" "private_s3_vod_distribution" {
    comment = "Private S3 VODs"

    default_cache_behavior {
        allowed_methods = ["GET", "HEAD", "OPTIONS"]
        cached_methods = ["GET", "HEAD"]
        compress = true
        default_ttl = 86400

        forwarded_values {
            query_string = false

            cookies {
                forward = "none"
            }
        }

        min_ttl = 3600
        max_ttl = 604800
        target_origin_id = "s3-bucket-origin"
        trusted_key_groups = [
            aws_cloudfront_key_group.private_s3_vod_cloudfront_key_group.id
        ]

        viewer_protocol_policy = "redirect-to-https"
    }

    enabled = true
    is_ipv6_enabled = true
    http_version = "http2"
    price_class = "PriceClass_All"

    tags = {
        "vods" = "private_cloudfront"
    }

    restrictions {
        geo_restriction {
            restriction_type = "none"
        }
    }

    viewer_certificate {
        cloudfront_default_certificate = true
    }

    origin {
        domain_name = aws_s3_bucket.vod_storage_bucket.bucket_domain_name
        origin_id = "s3-bucket-origin"
        s3_origin_config {
            origin_access_identity = aws_cloudfront_origin_access_identity.private_vod_access_identity.cloudfront_access_identity_path
        }
    }
}

resource "aws_cloudfront_distribution" "public_s3_vod_distribution" {
    comment = "Public S3 VODs"
    
    default_cache_behavior {
        allowed_methods = ["GET", "HEAD", "OPTIONS"]
        cached_methods = ["GET", "HEAD"]
        compress = true
        default_ttl = 86400

        forwarded_values {
            query_string = false

            cookies {
                forward = "none"
            }
        }

        min_ttl = 3600
        max_ttl = 604800
        target_origin_id = "s3-bucket-origin"
        viewer_protocol_policy = "redirect-to-https"
    }

    enabled = true
    is_ipv6_enabled = true
    http_version = "http2"
    price_class = "PriceClass_All"

    tags = {
        "vods" = "public_cloudfront"
    }

    restrictions {
        geo_restriction {
            restriction_type = "none"
        }
    }

    viewer_certificate {
        cloudfront_default_certificate = true
    }

    origin {
        domain_name = aws_s3_bucket.vod_storage_bucket.bucket_domain_name
        origin_id = "s3-bucket-origin"
        s3_origin_config {
            origin_access_identity = aws_cloudfront_origin_access_identity.public_vod_access_identity.cloudfront_access_identity_path
        }
    }
}

resource "aws_cloudfront_distribution" "private_s3_blob_distribution" {
    comment = "Private S3 Blob"

    default_cache_behavior {
        allowed_methods = ["GET", "HEAD", "OPTIONS"]
        cached_methods = ["GET", "HEAD"]
        compress = true
        default_ttl = 86400

        forwarded_values {
            query_string = false

            cookies {
                forward = "none"
            }
        }

        min_ttl = 3600
        max_ttl = 604800
        target_origin_id = "s3-bucket-origin"
        trusted_key_groups = [
            aws_cloudfront_key_group.private_s3_vod_cloudfront_key_group.id
        ]

        viewer_protocol_policy = "redirect-to-https"
    }

    enabled = true
    is_ipv6_enabled = true
    http_version = "http2"
    price_class = "PriceClass_All"

    tags = {
        "blobs" = "private_cloudfront"
    }

    restrictions {
        geo_restriction {
            restriction_type = "none"
        }
    }

    viewer_certificate {
        cloudfront_default_certificate = true
    }

    origin {
        domain_name = aws_s3_bucket.blob_storage_bucket.bucket_domain_name
        origin_id = "s3-bucket-origin"
        s3_origin_config {
            origin_access_identity = aws_cloudfront_origin_access_identity.private_blob_access_identity.cloudfront_access_identity_path
        }
    }
}