variable "redshift_user" {
    type = string
}

variable "redshift_password" {
    type = string
}

variable "redshift_subnets" {
    type = list(string)
}

variable "redshift_security_groups" {
    type = list(string)
}

variable "db_glue_connection_name" {
    type = string
}

variable "glue_subnet" {
    type = string
}

variable "bucket_suffix" {
    type = string
}

variable "db_secret_id" {
    type = string
}

variable "db_endpoint" {
    type = string
}