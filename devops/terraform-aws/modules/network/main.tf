resource "aws_vpc" "primary" {
    cidr_block = "10.0.0.0/16"
    instance_tenancy = "default"

    enable_dns_support = true
    enable_dns_hostnames = true
}

resource "aws_internet_gateway" "primary_gateway" {
    vpc_id = aws_vpc.primary.id
}

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
        ipv6_cidr_blocks = ["::/0"]
    }

    egress {
        from_port = 0
        to_port = 0
        protocol = "-1"
        cidr_blocks = ["0.0.0.0/0"]
        ipv6_cidr_blocks = ["::/0"]
    }
}

resource "aws_subnet" "k8s_subnet_private_a" {
    vpc_id = aws_vpc.primary.id
    availability_zone = "us-east-2a"
    cidr_block = "10.0.1.0/28"
}

resource "aws_subnet" "k8s_subnet_private_c" {
    vpc_id = aws_vpc.primary.id
    availability_zone = "us-east-2c"
    cidr_block = "10.0.1.32/28"
}

resource "aws_subnet" "k8s_subnet_public_a" {
    vpc_id = aws_vpc.primary.id
    availability_zone = "us-east-2a"
    cidr_block = "10.0.2.0/28"
}

resource "aws_subnet" "k8s_subnet_public_c" {
    vpc_id = aws_vpc.primary.id
    availability_zone = "us-east-2c"
    cidr_block = "10.0.2.32/28"
}


resource "aws_eip" "primary_nat_eip_a" {
    vpc = true
}

resource "aws_nat_gateway" "primary_nat_a" {
    allocation_id = aws_eip.primary_nat_eip_a.id
    connectivity_type = "public"
    subnet_id = aws_subnet.k8s_subnet_public_a.id

    depends_on = [ aws_internet_gateway.primary_gateway ]
}

resource "aws_eip" "primary_nat_eip_c" {
    vpc = true
}

resource "aws_nat_gateway" "primary_nat_c" {
    allocation_id = aws_eip.primary_nat_eip_c.id
    connectivity_type = "public"
    subnet_id = aws_subnet.k8s_subnet_public_c.id

    depends_on = [ aws_internet_gateway.primary_gateway ]
}

resource "aws_subnet" "fargate_subnet_private_a" {
    vpc_id = aws_vpc.primary.id
    availability_zone = "us-east-2a"
    cidr_block = "10.0.16.0/24"

    tags = {
        "kubernetes.io/cluster/primary-eks-cluster" = "shared"
    }
}

resource "aws_subnet" "fargate_subnet_private_c" {
    vpc_id = aws_vpc.primary.id
    availability_zone = "us-east-2c"
    cidr_block = "10.0.48.0/24"

    tags = {
        "kubernetes.io/cluster/primary-eks-cluster" = "shared"
    }
}

resource "aws_route_table" "public_route_table" {
    vpc_id = aws_vpc.primary.id

    route {
        cidr_block = "0.0.0.0/0"
        gateway_id = aws_internet_gateway.primary_gateway.id
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

resource "aws_route_table" "private_route_table_a" {
    vpc_id = aws_vpc.primary.id

    route {
        cidr_block = "0.0.0.0/0"
        gateway_id = aws_nat_gateway.primary_nat_a.id
    }
}

resource "aws_route_table" "private_route_table_c" {
    vpc_id = aws_vpc.primary.id

    route {
        cidr_block = "0.0.0.0/0"
        gateway_id = aws_nat_gateway.primary_nat_c.id
    }
}

resource "aws_route_table_association" "k8s_private_rt_subnet_a" {
    route_table_id = aws_route_table.private_route_table_a.id
    subnet_id = aws_subnet.k8s_subnet_private_a.id
}

resource "aws_route_table_association" "k8s_private_rt_subnet_c" {
    route_table_id = aws_route_table.private_route_table_c.id
    subnet_id = aws_subnet.k8s_subnet_private_c.id
}

resource "aws_route_table_association" "fargate_private_rt_subnet_a" {
    route_table_id = aws_route_table.private_route_table_a.id
    subnet_id = aws_subnet.fargate_subnet_private_a.id
}

resource "aws_route_table_association" "fargate_private_rt_subnet_c" {
    route_table_id = aws_route_table.private_route_table_c.id
    subnet_id = aws_subnet.fargate_subnet_private_c.id
}

output "database_subnets" {
    value = [aws_subnet.database_subnet_a.id, aws_subnet.database_subnet_c.id]
}

output "database_security_groups" {
    value = [aws_security_group.database_security_group.id]
}

output "k8s_subnets" {
    value = [
        aws_subnet.k8s_subnet_public_a.id,
        aws_subnet.k8s_subnet_public_c.id,
        aws_subnet.k8s_subnet_private_a.id,
        aws_subnet.k8s_subnet_private_c.id
    ]
}

output "default_fargate_subnets" {
    value = [
        aws_subnet.fargate_subnet_private_a.id,
        aws_subnet.fargate_subnet_private_c.id
    ]
}
