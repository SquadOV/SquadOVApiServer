#!/bin/bash
cd $1

export TF_VAR_postgres_user=$POSTGRES_USER
export TF_VAR_postgres_password=$POSTGRES_PASSWORD
export TF_VAR_postgres_instance_name=$POSTGRES_INSTANCE_NAME
export TF_VAR_redshift_user=$REDSHIFT_USER
export TF_VAR_redshift_password=$REDSHIFT_PASSWORD


terraform init
terraform apply -target="module.network"
terraform apply