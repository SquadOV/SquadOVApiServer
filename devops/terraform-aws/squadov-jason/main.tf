terraform {
    required_providers {
        aws = {
            source  = "hashicorp/aws"
            version = "~> 3.70"
        }
    }

    backend "s3" {
        bucket = "squadov-aws-tf-dev-state-jason"
        key = "tfstate"
        region = "us-east-2"
        profile = "terraformdev"
    }

    required_version = ">= 1.0.2"
}

provider "aws" {
    region              = "us-east-2"
    profile             = "terraformdev"
    allowed_account_ids = [ 800367723057 ]
}

module "network" {
    source = "../modules/network"

    domain_prefix = "jasondev."
}

module "iam" {
    source = "../modules/iam"
}

module "storage" {
    source = "../modules/storage"

    bucket_suffix = "-dev-jason"
    cloudfront_suffix = "-dev-jason"
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
    glue_subnet = module.network.private_k8s_subnets[0]
    redis_instance_type = "cache.t4g.micro"
}