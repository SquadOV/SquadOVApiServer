resource "aws_subnet" "database_subnet_a" {
    vpc_id = aws_vpc.primary.id
    availability_zone = "us-east-2a"
    cidr_block = "10.0.0.0/28"
    map_public_ip_on_launch = true
}

resource "aws_subnet" "database_subnet_c" {
    vpc_id = aws_vpc.primary.id
    availability_zone = "us-east-2c"
    cidr_block = "10.0.0.32/28"
    map_public_ip_on_launch = true
}

resource "aws_security_group" "database_security_group" {
    name = "database-security-group"
    description = "Security group for the primary VPC for the database."
    vpc_id = aws_vpc.primary.id

    ingress {
        description = "PostgreSQL connections."
        from_port = 5432
        to_port = 5432
        protocol = "tcp"
        cidr_blocks = ["0.0.0.0/0"]
    }

    egress {
        from_port = 0
        to_port = 0
        protocol = "-1"
        cidr_blocks = ["0.0.0.0/0"]
    }
}

resource "aws_route_table_association" "database_rt_subnet_a" {
    route_table_id = aws_route_table.public_route_table.id
    subnet_id = aws_subnet.database_subnet_a.id
}

resource "aws_route_table_association" "database_rt_subnet_c" {
    route_table_id = aws_route_table.public_route_table.id
    subnet_id = aws_subnet.database_subnet_c.id
}

resource "aws_route_table_association" "k8s_public_rt_subnet_a" {
    route_table_id = aws_route_table.public_route_table.id
    subnet_id = aws_subnet.k8s_subnet_public_a.id
}

resource "aws_route_table_association" "k8s_public_rt_subnet_c" {
    route_table_id = aws_route_table.public_route_table.id
    subnet_id = aws_subnet.k8s_subnet_public_c.id
}

output "database_subnets" {
    value = [aws_subnet.database_subnet_a.id, aws_subnet.database_subnet_c.id]
}

output "database_security_groups" {
    value = [aws_security_group.database_security_group.id]
}