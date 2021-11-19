#!/bin/bash
cd $1

export TF_VAR_postgres_user=$POSTGRES_USER
export TF_VAR_postgres_password=$POSTGRES_PASSWORD
export TF_VAR_postgres_instance_name=$POSTGRES_INSTANCE_NAME

terraform init -reconfigure -backend-config="access_key=<your access key>" -backend-config="secret_key=<your secret key>"
terraform apply