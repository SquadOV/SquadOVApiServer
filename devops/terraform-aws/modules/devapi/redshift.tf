data "aws_subnet" "glue_subnet" {
    id = var.glue_subnet
}

resource "aws_redshift_subnet_group" "rs_subnet_group" {
    name ="squadov-rs-cluster-subnet-group"
    subnet_ids = var.redshift_subnets
}

resource "aws_iam_role" "redshift_role" {
    name = "redshift-role"
    assume_role_policy = <<POLICY
{
    "Version": "2012-10-17",
    "Statement": [
        {
            "Effect": "Allow",
            "Principal": {
                "Service": [
                    "redshift.amazonaws.com"
                ]
            },
            "Action": "sts:AssumeRole"
        }
    ]
}
POLICY
}


resource "aws_iam_policy" "redshift_extra_policy" {
    name = "redshift_extra_policy"
    description = "Extra policy for Redshift Role."

    policy = <<EOF
{
    "Version": "2012-10-17",
    "Statement": [
        {
            "Effect": "Allow",
            "Action": [
                "kms:GenerateDataKey",
                "kms:Decrypt",
                "kms:Encrypt",
                "s3:Get*",
                "s3:List*",
                "s3:Put*",
                "s3:Delete*",
                "glue:*"
            ],
            "Resource": "*"
        }
    ]
}
EOF
}

resource "aws_iam_role_policy_attachment" "redshift_extra_policy_attach" {
    role = aws_iam_role.redshift_role.name
    policy_arn = aws_iam_policy.redshift_extra_policy.arn
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
    iam_roles = [
        aws_iam_role.redshift_role.arn
    ]
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