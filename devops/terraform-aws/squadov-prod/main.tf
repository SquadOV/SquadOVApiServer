terraform {
    required_providers {
        aws = {
            source  = "hashicorp/aws"
            version = "~> 4.13"
        }
    }

    backend "s3" {
        bucket = "squadov-prod-tf-state"
        key = "tfstate"
        region = "us-east-2"
        profile = "terraform"
    }

    required_version = ">= 1.0.2"
}

provider "aws" {
    region              = "us-east-2"
    profile             = "terraform"
    allowed_account_ids = [ 214663929182 ]
}

module "network" {
    source = "../modules/network"

    domain_prefix = ""
}

module "db" {
    source = "../modules/db"

    postgres_instance_name = var.postgres_instance_name
    postgres_user = var.postgres_user
    postgres_password = var.postgres_password
    postgres_db_size = 1024
    postgres_max_db_size = 65536
    postgres_instance_type = "db.m6g.8xlarge"
    postgres_db_subnets = module.network.database_subnets
    postgres_db_security_groups = module.network.database_security_groups
    glue_subnet = module.network.private_k8s_subnets[0]

    secondary_instance_name = "${var.postgres_instance_name}-secondary"
    secondary_db_size = 1024
    secondary_max_db_size = 5120
    secondary_instance_type = "db.m6g.8xlarge"

    redis_instance_type = "cache.t4g.medium"
}

module "storage" {
    source = "../modules/storage"

    bucket_suffix = "-prod"
    cloudfront_suffix = ""
}

module "combatlog" {
    source = "../modules/combatlog"

    combatlog_bucket_name = module.storage.combatlog_bucket_name

    lambda_subnets = module.network.lambda_subnets
    lambda_security_groups = module.network.lambda_security_groups

    db_host = module.db.db_host
    db_secret = module.db.db_secret

    wow_shards = 16
    amqp_url = var.amqp_url
}

module "iam" {
    source = "../modules/iam"

    resource_suffix = "-prod"
    api_gateway_id = module.combatlog.api_gateway_id
}

module "k8s" {
    source = "../modules/k8s"

    public_k8s_subnets = module.network.public_k8s_subnets
    private_k8s_subnets = module.network.private_k8s_subnets
    default_fargate_subnets = module.network.default_fargate_subnets
}