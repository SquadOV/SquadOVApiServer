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

module "iam" {
    source = "../modules/iam"
    resource_suffix = "-dev-mike"
}

module "storage" {
    source = "../modules/storage"

    bucket_suffix = "-dev-mike"
    cloudfront_suffix = "-dev-mike"
}

module "combatlog" {
    source = "../modules/combatlog"
}