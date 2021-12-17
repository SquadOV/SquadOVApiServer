terraform {
    required_providers {
        aws = {
            source  = "hashicorp/aws"
            version = "~> 3.48"
        }
    }

    backend "s3" {
        bucket = "squadov-aws-tf-dev-state"
        key = "tfstate"
        region = "us-east-2"
        profile = "terraformdev"
    }

    required_version = ">= 1.0.2"
}

provider "aws" {
    region              = "us-east-2"
    profile             = "terraformdev"
    allowed_account_ids = [ 897997503846 ]
}

module "iam" {
    source = "../modules/iam"
}

module "storage" {
    source = "../modules/storage"

    bucket_suffix = "-dev"
}