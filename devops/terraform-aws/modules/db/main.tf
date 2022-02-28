data "aws_rds_certificate" "rds_cert" {
    latest_valid_till = true
}

resource "aws_db_parameter_group" "primary_db_parameters" {
    name = "primary-squadov-db-parameters"
    family = "postgres12"

    parameter {
        name = "rds.force_ssl"
        value = "1"
    }

    parameter {
        name = "maintenance_work_mem"
        value = "1048576"
    }

/*
    parameter {
        name = "maintenance_work_mem"
        value = "2097152"
    }

    parameter {
        apply_method = "pending-reboot"
        name = "shared_buffers"
        value = "524288"
    }

    parameter {
        name = "autovacuum"
        value = "0"
    }
    
    parameter {
        apply_method = "pending-reboot"
        name = "wal_buffers"
        value = "8192"
    }

    parameter {
        name = "checkpoint_timeout"
        value = "1800"
    }

    parameter {
        name = "max_wal_size"
        value = "8192"
    }
*/
}

resource "aws_db_subnet_group" "primary_db_subnet" {
    name = "primary-squadov-db-subnet-group"
    subnet_ids = var.postgres_db_subnets
}

resource "aws_iam_role" "rds_monitoring_role" {
    name = "rds-monitoring-role"

    assume_role_policy = <<POLICY
{
    "Version": "2012-10-17",
    "Statement": [
        {
            "Effect": "Allow",
            "Principal": {
                "Service": [
                    "monitoring.rds.amazonaws.com"
                ]
            },
            "Action": "sts:AssumeRole"
        }
    ]
}
POLICY
}

resource "aws_iam_role_policy_attachment" "rds_role_AmazonRDSEnhancedMonitoringRole" {
    policy_arn = "arn:aws:iam::aws:policy/service-role/AmazonRDSEnhancedMonitoringRole"
    role       = aws_iam_role.rds_monitoring_role.name
}

resource "aws_db_instance" "primary_db" {
    allocated_storage = var.postgres_db_size
    apply_immediately = false
    max_allocated_storage = var.postgres_max_db_size
    backup_retention_period = 14
    backup_window = "02:00-02:30"
    ca_cert_identifier = data.aws_rds_certificate.rds_cert.id
    db_subnet_group_name = aws_db_subnet_group.primary_db_subnet.name
    delete_automated_backups = false
    deletion_protection = true
    enabled_cloudwatch_logs_exports = [ "postgresql", "upgrade" ]
    engine = "postgres"
    engine_version = "12.7"
    identifier = var.postgres_instance_name
    instance_class = var.postgres_instance_type
    name = "squadov"
    password = var.postgres_password
    parameter_group_name = aws_db_parameter_group.primary_db_parameters.name
    publicly_accessible = true
    storage_encrypted = true
    storage_type = "gp2"
    username = var.postgres_user
    vpc_security_group_ids = var.postgres_db_security_groups

    monitoring_interval = 30
    monitoring_role_arn = aws_iam_role.rds_monitoring_role.arn

    performance_insights_enabled = true
    performance_insights_retention_period = 7

    tags = {
        "db" = "primary"
    }
}

resource "aws_iam_role" "rds_proxy_role" {
    name = "rds-proxy-role"

    assume_role_policy = <<POLICY
{
    "Version": "2012-10-17",
    "Statement": [
        {
            "Effect": "Allow",
            "Principal": {
                "Service": [
                    "rds.amazonaws.com"
                ]
            },
            "Action": "sts:AssumeRole"
        }
    ]
}
POLICY
}

resource "aws_iam_policy" "rds_proxy_policy" {
    name = "rds-proxy-policy"
    description = "Policy to allow RDS proxy to access le secrets."

    policy = <<EOF
{
    "Version": "2012-10-17",
    "Statement": [
        {
            "Effect": "Allow",
            "Action": [
                "secretsmanager:GetSecretValue"
            ],
            "Resource": "*"
        },
        {
            "Effect": "Allow",
            "Action": [
                "kms:Decrypt"
            ],
            "Resource": "*",
            "Condition": {
                "StringEquals": {
                    "kms:ViaService": "secretsmanager.us-east-2.amazonaws.com"
                }
            }
        }
    ]
}
EOF
}

resource "aws_iam_role_policy_attachment" "rds_proxy_policy_attachment" {
    role = aws_iam_role.rds_proxy_role.name
    policy_arn = aws_iam_policy.rds_proxy_policy.arn
}

resource "aws_secretsmanager_secret" "primary_db_credentials_secret" {
    name = "primary_db_credentials_secret"
}

output "db_secret_id" {
    value = aws_secretsmanager_secret.primary_db_credentials_secret.id
}

output "db_endpoint" {
    value = aws_db_instance.primary_db.endpoint
}

resource "aws_secretsmanager_secret_version" "primary_db_credentials_secret_ver" {
    secret_id     = aws_secretsmanager_secret.primary_db_credentials_secret.id
    secret_string = jsonencode({
        "username" = var.postgres_user,
        "password" = var.postgres_password,
        "engine"="postgres",
        "host" = aws_db_instance.primary_db.address
        "port" = 5432,
        "dbInstanceIdentifier" = aws_db_instance.primary_db.id
    })
}

resource "aws_db_proxy" "primary_db_proxy" {
    name = "${var.postgres_instance_name}-proxy"
    debug_logging = false
    engine_family = "POSTGRESQL"
    idle_client_timeout = 600
    require_tls = true
    role_arn = aws_iam_role.rds_proxy_role.arn
    vpc_subnet_ids = var.postgres_db_subnets

    auth {
        auth_scheme = "SECRETS"
        iam_auth = "DISABLED"
        secret_arn  = aws_secretsmanager_secret.primary_db_credentials_secret.arn
    }
}

resource "aws_db_proxy_default_target_group" "primary_db_proxy_target_group" {
    db_proxy_name = aws_db_proxy.primary_db_proxy.name

    connection_pool_config {
        connection_borrow_timeout    = 120
        init_query                   = "SET TIME ZONE 'GMT'"
        max_connections_percent      = 90
        max_idle_connections_percent = 50
    }
}

resource "aws_db_proxy_target" "primary_db_proxy_target" {
    db_instance_identifier = aws_db_instance.primary_db.id
    db_proxy_name          = aws_db_proxy.primary_db_proxy.name
    target_group_name      = aws_db_proxy_default_target_group.primary_db_proxy_target_group.name
}

resource "aws_db_proxy_endpoint" "primary_db_proxy_endpoint" {
    db_proxy_name          = aws_db_proxy.primary_db_proxy.name
    db_proxy_endpoint_name = "${var.postgres_instance_name}-proxy-endpoint"
    vpc_subnet_ids         = var.postgres_db_subnets
    target_role            = "READ_WRITE"
}

resource "aws_elasticache_subnet_group" "redis_subnet" {
    name = "redis-subnet-group"
    subnet_ids = var.postgres_db_subnets
}

resource "aws_elasticache_cluster" "redis" {
    cluster_id = "squadov-redis"
    engine = "redis"
    node_type = var.redis_instance_type
    num_cache_nodes = 1
    parameter_group_name = "default.redis6.x"
    engine_version = "6.x"
    port = 6379
    subnet_group_name = aws_elasticache_subnet_group.redis_subnet.name
}