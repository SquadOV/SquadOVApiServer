#!/bin/bash
set -e
cd $1

export TF_VAR_postgres_user=$POSTGRES_USER
export TF_VAR_postgres_password=$POSTGRES_PASSWORD
export TF_VAR_postgres_instance_name=$POSTGRES_INSTANCE_NAME
export TF_VAR_environment="dev"
export TF_VAR_region="us-east-2"
export TF_VAR_user=""

terraform init -reconfigure -backend-config="access_key=<your access key>" -backend-config="secret_key=<your secret key>"
terraform fmt
terraform plan