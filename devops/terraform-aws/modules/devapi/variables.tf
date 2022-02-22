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