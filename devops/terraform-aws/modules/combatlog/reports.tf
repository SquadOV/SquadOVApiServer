resource "aws_efs_file_system" "combat_logs_efs" {
    tags = {
        name = "Combat Logs EFS"
    }
}

resource "aws_efs_mount_target" "combat_logs_efs_mt" {
    file_system_id = aws_efs_file_system.combat_logs_efs.id

    for_each = toset(var.lambda_subnets)
    subnet_id = each.key
    security_groups = var.lambda_security_groups
}

resource "aws_efs_access_point" "combat_logs_efs_ap" {
    file_system_id = aws_efs_file_system.combat_logs_efs.id

    root_directory {
        path = "/lambda"
        creation_info {
            owner_gid = 1000
            owner_uid = 1000
            permissions = "777"
        }
    }

    posix_user {
        gid = 1000
        uid = 1000
    }
}

resource "aws_lambda_function" "combat_log_reports_lambda" {
    function_name = "combat-log-reports-lambda"
    role = aws_iam_role.lambda_role.arn

    filename = "../../aws/lambda/reports.zip"
    source_code_hash = filebase64sha256("../../aws/lambda/reports.zip")

    handler = "not.used"
    memory_size = 128
    package_type = "Zip"
    reserved_concurrent_executions = 256
    runtime = "provided.al2"
    timeout = 360

    tags = {
        "lambda" = "combatlog-reports"
    }

    vpc_config {
        subnet_ids = var.lambda_subnets
        security_group_ids = var.lambda_security_groups
    }

    file_system_config {
        arn = aws_efs_access_point.combat_logs_efs_ap.arn

        local_mount_path = "/mnt/efs"
    }

    environment {
        variables = {
            "SQUADOV_AWS_REGION" = "us-east-2"
            "SQUADOV_EFS_DIRECTORY" = "/mnt/efs"
            "SQUADOV_LAMBDA_DB_SECRET" = var.db_secret
            "SQUADOV_LAMBDA_DBHOST" = var.db_host
        }
    }

    depends_on = [aws_efs_mount_target.combat_logs_efs_mt]
}