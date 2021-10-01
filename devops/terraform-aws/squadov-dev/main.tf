terraform {
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 3.48"
    }
  }

  backend "s3" {
    bucket  = "squadov-aws-tf-dev-st-state"
    key     = "state.tfstate"
    region  = "us-east-2"
    profile = "<profile from aws_terraform_dev.profile>"
  }

  required_version = ">= 1.0.2"
}

provider "aws" {
  region              = "us-east-2"
  profile             = "<profile from aws_terraform_dev.profile>"
  allowed_account_ids = [897997503846]
}

module "iam" {
  source      = "../modules/iam"
  environment = var.environment
  user        = var.user
}

module "storage" {
  source      = "../modules/storage"
  environment = var.environment
  user        = var.user
}