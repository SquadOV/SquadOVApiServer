data "aws_subnet" "glue_subnet" {
    id = var.glue_subnet
}

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

resource "aws_glue_connection" "redshift_connection" {
    connection_properties = {
        JDBC_CONNECTION_URL = "jdbc:redshift://${aws_redshift_cluster.rs_cluster.endpoint}/squadov"
        PASSWORD = var.redshift_password
        USERNAME = var.redshift_user
    }

    name = "glue-redshift-connection"

    physical_connection_requirements {
        availability_zone      = data.aws_subnet.glue_subnet.availability_zone
        security_group_id_list = var.redshift_security_groups
        subnet_id              = data.aws_subnet.glue_subnet.id
    }
}