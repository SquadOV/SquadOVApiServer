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
    }

    ingress {
        description = "Redshift connections."
        from_port = 5439
        to_port = 5439
        protocol = "tcp"
        cidr_blocks = ["0.0.0.0/0"]
    }

    ingress {
        description = "Internal"
        from_port = 0
        to_port = 0
        protocol = "-1"
        self = true
    }

    egress {
        from_port = 0
        to_port = 0
        protocol = "-1"
        cidr_blocks = ["0.0.0.0/0"]
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

resource "aws_subnet" "k8s_subnet_public_a_v2" {
    vpc_id = aws_vpc.primary.id
    availability_zone = "us-east-2a"
    cidr_block = "10.0.3.0/24"
    
    tags = {
        "kubernetes.io/cluster/primary-eks-cluster" = "shared"
        "kubernetes.io/role/elb" = "1"
    }
}

resource "aws_subnet" "k8s_subnet_public_c_v2" {
    vpc_id = aws_vpc.primary.id
    availability_zone = "us-east-2c"
    cidr_block = "10.0.4.0/24"
    
    tags = {
        "kubernetes.io/cluster/primary-eks-cluster" = "shared"
        "kubernetes.io/role/elb" = "1"
    }
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

resource "aws_eip" "primary_nat_eip_a_v2" {
    vpc = true
}

resource "aws_nat_gateway" "primary_nat_a_v2" {
    allocation_id = aws_eip.primary_nat_eip_a_v2.id
    connectivity_type = "public"
    subnet_id = aws_subnet.k8s_subnet_public_a_v2.id

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

resource "aws_eip" "primary_nat_eip_c_v2" {
    vpc = true
}

resource "aws_nat_gateway" "primary_nat_c_v2" {
    allocation_id = aws_eip.primary_nat_eip_c_v2.id
    connectivity_type = "public"
    subnet_id = aws_subnet.k8s_subnet_public_c_v2.id

    depends_on = [ aws_internet_gateway.primary_gateway ]
}

resource "aws_subnet" "fargate_subnet_private_a" {
    vpc_id = aws_vpc.primary.id
    availability_zone = "us-east-2a"
    cidr_block = "10.0.16.0/24"
}

resource "aws_subnet" "fargate_subnet_private_c" {
    vpc_id = aws_vpc.primary.id
    availability_zone = "us-east-2c"
    cidr_block = "10.0.48.0/24"
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

resource "aws_route_table_association" "k8s_public_rt_subnet_a_v2" {
    route_table_id = aws_route_table.public_route_table.id
    subnet_id = aws_subnet.k8s_subnet_public_a_v2.id
}

resource "aws_route_table_association" "k8s_public_rt_subnet_c_v2" {
    route_table_id = aws_route_table.public_route_table.id
    subnet_id = aws_subnet.k8s_subnet_public_c_v2.id
}

resource "aws_vpc_endpoint" "s3_endpoint_us_east_2" {
    vpc_id = aws_vpc.primary.id
    service_name = "com.amazonaws.us-east-2.s3"
    vpc_endpoint_type = "Gateway"
}

resource "aws_route_table" "private_route_table_a" {
    vpc_id = aws_vpc.primary.id

    route {
        cidr_block = "0.0.0.0/0"
        nat_gateway_id = aws_nat_gateway.primary_nat_a.id
    }
}

resource "aws_route_table" "private_route_table_c" {
    vpc_id = aws_vpc.primary.id

    route {
        cidr_block = "0.0.0.0/0"
        nat_gateway_id = aws_nat_gateway.primary_nat_c.id
    }
}

resource "aws_vpc_endpoint_route_table_association" "s3_private_rt_a" {
    route_table_id = aws_route_table.private_route_table_a.id
    vpc_endpoint_id = aws_vpc_endpoint.s3_endpoint_us_east_2.id
}

resource "aws_vpc_endpoint_route_table_association" "s3_private_rt_c" {
    route_table_id = aws_route_table.private_route_table_c.id
    vpc_endpoint_id = aws_vpc_endpoint.s3_endpoint_us_east_2.id
}

resource "aws_vpc_endpoint_route_table_association" "s3_public_rt" {
    route_table_id = aws_route_table.public_route_table.id
    vpc_endpoint_id = aws_vpc_endpoint.s3_endpoint_us_east_2.id
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

output "public_k8s_subnets" {
    value = [
        aws_subnet.k8s_subnet_public_a.id,
        aws_subnet.k8s_subnet_public_c.id
    ]
}

output "private_k8s_subnets" {
    value = [
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

resource "aws_acm_certificate" "domain_certificates" {
    domain_name = "${var.domain_prefix}squadov.gg"
    subject_alternative_names = [ "*.${var.domain_prefix}squadov.gg" ]
    validation_method = "DNS"
}