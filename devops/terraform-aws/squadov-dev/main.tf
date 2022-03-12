terraform {
    required_providers {
        aws = {
            source  = "hashicorp/aws"
            version = "~> 3.70"
        }
    }

    backend "s3" {
        bucket = "squadov-aws-tf-dev-state-mike"
        key = "tfstate"
        region = "us-east-2"
        profile = "terraformdev"
    }

    required_version = ">= 1.0.2"
}

provider "aws" {
    region              = "us-east-2"
    profile             = "terraformdev"
    allowed_account_ids = [ 778673984203 ]
}

module "network" {
    source = "../modules/network"

    domain_prefix = "mikedev."
}

module "db" {
    source = "../modules/db"

    postgres_instance_name = var.postgres_instance_name
    postgres_user = var.postgres_user
    postgres_password = var.postgres_password
    postgres_db_size = 20
    postgres_max_db_size = 40
    postgres_instance_type = "db.t4g.micro"
    postgres_db_subnets = module.network.database_subnets
    postgres_db_security_groups = module.network.database_security_groups

    redis_instance_type = "cache.t4g.micro"
}


module "storage" {
    source = "../modules/storage"

    bucket_suffix = "-dev-mike"
    cloudfront_suffix = "-dev-mike"
}

module "combatlog" {
    source = "../modules/combatlog"

    combatlog_bucket_arn = module.storage.combatlog_bucket_arn
    combatlog_bucket_id = module.storage.combatlog_bucket_id

    lambda_subnets = module.network.lambda_subnets
    lambda_security_groups = module.network.lambda_security_groups

    db_host = module.db.db_host
    db_secret = module.db.db_secret
}

module "iam" {
    source = "../modules/iam"
    resource_suffix = "-dev-mike"
    api_gateway_id = module.combatlog.api_gateway_id
}