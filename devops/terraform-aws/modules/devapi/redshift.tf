resource "aws_redshift_subnet_group" "rs_subnet_group" {
    name ="squadov-rs-cluster-subnet-group"
    subnet_ids = var.redshift_subnets
}

resource "aws_redshift_cluster" "rs_cluster" {
    cluster_identifier = "squadov-rs-cluster"
    database_name = "squadov"
    node_type = "dc2.large"
    cluster_type = "single-node"
    master_username = var.redshift_user
    master_password = var.redshift_password
    vpc_security_group_ids = var.redshift_security_groups
    cluster_subnet_group_name = aws_redshift_subnet_group.rs_subnet_group.name
    availability_zone = "us-east-2c"
    publicly_accessible = true
    encrypted = true
}