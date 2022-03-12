variable "combatlog_bucket_arn" {
    type = string
}

variable "combatlog_bucket_id" {
    type = string
}

variable "lambda_subnets" {
    type = list(string)
}

variable "lambda_security_groups" {
    type = list(string)
}

variable "db_host" {
    type = string
}

variable "db_secret" {
    type = string
}