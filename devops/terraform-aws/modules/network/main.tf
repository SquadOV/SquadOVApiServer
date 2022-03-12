resource "aws_vpc" "primary" {
    cidr_block = "10.0.0.0/16"
    instance_tenancy = "default"

    enable_dns_support = true
    enable_dns_hostnames = true
}

resource "aws_default_security_group" "primary_sg" {
    vpc_id = aws_vpc.primary.id

    ingress {
        protocol = "tcp"
        from_port = 5432
        to_port = 5432
        security_groups = [
            aws_default_security_group.lambda_sg.id,
            aws_security_group.lambda_security_group.id
        ]
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

    ingress {
        description = "Redis"
        from_port = 6379
        to_port = 6379
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

resource "aws_internet_gateway" "primary_gateway" {
    vpc_id = aws_vpc.primary.id
}

resource "aws_route_table" "public_route_table" {
    vpc_id = aws_vpc.primary.id

    route {
        cidr_block = "0.0.0.0/0"
        gateway_id = aws_internet_gateway.primary_gateway.id
    }
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

resource "aws_acm_certificate" "domain_certificates" {
    domain_name = "${var.domain_prefix}squadov.gg"
    subject_alternative_names = [ "*.${var.domain_prefix}squadov.gg" ]
    validation_method = "DNS"
}

output "primary_vpc" {
    value = aws_vpc.primary.id
}