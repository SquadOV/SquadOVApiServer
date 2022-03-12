resource "aws_vpc" "lambda" {
    cidr_block = "192.168.0.0/16"
    instance_tenancy = "default"
    enable_dns_support = true
    enable_dns_hostnames = true
}

resource "aws_default_security_group" "lambda_sg" {
    vpc_id = aws_vpc.lambda.id

    ingress {
        protocol = -1
        self = true
        from_port = 0
        to_port = 0
    }

    egress {
        from_port = 0
        to_port = 0
        protocol = "-1"
        cidr_blocks = ["0.0.0.0/0"]
    }
}

resource "aws_internet_gateway" "lambda_gateway" {
    vpc_id = aws_vpc.lambda.id
}

resource "aws_route_table" "lambda_public_route_table" {
    vpc_id = aws_vpc.lambda.id

    route {
        cidr_block = "0.0.0.0/0"
        gateway_id = aws_internet_gateway.lambda_gateway.id
    }
}

resource "aws_subnet" "lambda_subnet_public_a" {
    vpc_id = aws_vpc.lambda.id
    availability_zone = "us-east-2a"
    cidr_block = "192.168.0.0/18"
}

resource "aws_route_table_association" "lambda_public_rt_subnet_a" {
    route_table_id = aws_route_table.lambda_public_route_table.id
    subnet_id = aws_subnet.lambda_subnet_public_a.id
}

resource "aws_vpc_peering_connection" "lambda_primary" {
    peer_vpc_id = aws_vpc.primary.id
    vpc_id = aws_vpc.lambda.id
    auto_accept = true

    accepter {
        allow_remote_vpc_dns_resolution = true
    }

    requester {
        allow_remote_vpc_dns_resolution = true
    }
}

resource "aws_subnet" "lambda_subnet_a" {
    vpc_id = aws_vpc.lambda.id
    availability_zone = "us-east-2a"
    cidr_block = "192.168.64.0/18"
}

resource "aws_subnet" "lambda_subnet_c" {
    vpc_id = aws_vpc.lambda.id
    availability_zone = "us-east-2c"
    cidr_block = "192.168.192.0/18"
}

resource "aws_eip" "lambda_nat_eip_a" {
    vpc = true
}

resource "aws_nat_gateway" "lambda_nat_a" {
    allocation_id = aws_eip.lambda_nat_eip_a.id
    connectivity_type = "public"
    subnet_id = aws_subnet.lambda_subnet_public_a.id

    depends_on = [ aws_internet_gateway.lambda_gateway ]
}

resource "aws_route_table" "lambda_route_table_a" {
    vpc_id = aws_vpc.lambda.id

    route {
        cidr_block = aws_vpc.primary.cidr_block
        vpc_peering_connection_id = aws_vpc_peering_connection.lambda_primary.id
    }

    route {
        cidr_block = "0.0.0.0/0"
        nat_gateway_id = aws_nat_gateway.lambda_nat_a.id
    }
}

resource "aws_route_table_association" "lambda_rt_subnet_a" {
    route_table_id = aws_route_table.lambda_route_table_a.id
    subnet_id = aws_subnet.lambda_subnet_a.id
}


resource "aws_route_table" "lambda_route_table_c" {
    vpc_id = aws_vpc.lambda.id

    route {
        cidr_block = aws_vpc.primary.cidr_block
        vpc_peering_connection_id = aws_vpc_peering_connection.lambda_primary.id
    }

    route {
        cidr_block = "0.0.0.0/0"
        nat_gateway_id = aws_nat_gateway.lambda_nat_a.id
    }
}

resource "aws_route_table_association" "lambda_rt_subnet_c" {
    route_table_id = aws_route_table.lambda_route_table_c.id
    subnet_id = aws_subnet.lambda_subnet_c.id
}

resource "aws_security_group" "lambda_security_group" {
    name = "lambda-security-group"
    description = "Security group for the primary VPC for Lambda."
    vpc_id = aws_vpc.lambda.id

    ingress {
        from_port = 0
        to_port = 0
        protocol = "-1"
        cidr_blocks = ["0.0.0.0/0"]
    }

    egress {
        from_port = 0
        to_port = 0
        protocol = "-1"
        cidr_blocks = ["0.0.0.0/0"]
    }
}

resource "aws_vpc_endpoint" "secretsmanager_endpoint_us_east_2" {
    vpc_id = aws_vpc.lambda.id
    service_name = "com.amazonaws.us-east-2.secretsmanager"
    vpc_endpoint_type = "Interface"
    private_dns_enabled = true
    security_group_ids = [aws_security_group.lambda_security_group.id]
}

resource "aws_vpc_endpoint_subnet_association" "secrets_private_rt_a" {
    subnet_id = aws_subnet.lambda_subnet_a.id
    vpc_endpoint_id = aws_vpc_endpoint.secretsmanager_endpoint_us_east_2.id
}

resource "aws_vpc_endpoint_subnet_association" "secrets_private_rt_c" {
    subnet_id = aws_subnet.lambda_subnet_c.id
    vpc_endpoint_id = aws_vpc_endpoint.secretsmanager_endpoint_us_east_2.id
}

resource "aws_vpc_endpoint" "firehose_endpoint_us_east_2" {
    vpc_id = aws_vpc.lambda.id
    service_name = "com.amazonaws.us-east-2.kinesis-firehose"
    vpc_endpoint_type = "Interface"
    private_dns_enabled = true
    security_group_ids = [aws_security_group.lambda_security_group.id]
}

resource "aws_vpc_endpoint_subnet_association" "firehose_private_rt_a" {
    subnet_id = aws_subnet.lambda_subnet_a.id
    vpc_endpoint_id = aws_vpc_endpoint.firehose_endpoint_us_east_2.id
}

resource "aws_vpc_endpoint_subnet_association" "firehose_private_rt_c" {
    subnet_id = aws_subnet.lambda_subnet_c.id
    vpc_endpoint_id = aws_vpc_endpoint.firehose_endpoint_us_east_2.id
}

output "lambda_subnets" {
    value = [aws_subnet.lambda_subnet_a.id, aws_subnet.lambda_subnet_c.id]
}

output "lambda_security_groups" {
    value = [aws_security_group.lambda_security_group.id]
}