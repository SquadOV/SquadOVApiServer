terraform {
    required_providers {
        aws = {
            source  = "hashicorp/aws"
            version = "~> 3.70"
        }
    }

    backend "s3" {
        bucket = "squadov-aws-tf-dev-steven-state"
        key = "tfstate"
        region = "us-east-2"
        profile = "terraformdevsteven"
    }

    required_version = ">= 1.0.2"
}

provider "aws" {
    region              = "us-east-2"
    shared_credentials_file = "../../aws/aws_terraform_dev.profile"
    profile             = "terraformdevsteven"
    allowed_account_ids = [ 372337310825 ]
}

module "iam" {
    source = "../modules/iam"
}

module "storage" {
    source = "../modules/storage"

    bucket_suffix = "-dev-steven"
}