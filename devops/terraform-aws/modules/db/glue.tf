data "aws_subnet" "glue_subnet" {
    id = var.glue_subnet
}

resource "aws_glue_connection" "rds_pg_connection" {
    connection_properties = {
        JDBC_CONNECTION_URL = "jdbc:postgresql://${aws_db_instance.primary_db.endpoint}/squadov"
        PASSWORD = var.postgres_password
        USERNAME = var.postgres_user
    }

    name = "glue-rds-connection"

    physical_connection_requirements {
        availability_zone      = data.aws_subnet.glue_subnet.availability_zone
        security_group_id_list = var.postgres_db_security_groups
        subnet_id              = data.aws_subnet.glue_subnet.id
    }
}

output "rds_glue_connection_name" {
    value = aws_glue_connection.rds_pg_connection.name
}