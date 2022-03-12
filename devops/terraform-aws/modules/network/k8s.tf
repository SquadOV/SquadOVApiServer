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
    
    tags = {
        "kubernetes.io/cluster/primary-eks-cluster" = "shared"
        "kubernetes.io/role/elb" = "1"
    }
}

resource "aws_subnet" "k8s_subnet_public_c" {
    vpc_id = aws_vpc.primary.id
    availability_zone = "us-east-2c"
    cidr_block = "10.0.2.32/28"
    
    tags = {
        "kubernetes.io/cluster/primary-eks-cluster" = "shared"
        "kubernetes.io/role/elb" = "1"
    }
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

resource "aws_subnet" "fargate_subnet_private_a_v2" {
    vpc_id = aws_vpc.primary.id
    availability_zone = "us-east-2a"
    cidr_block = "10.0.17.0/24"
}

resource "aws_subnet" "fargate_subnet_private_c_v2" {
    vpc_id = aws_vpc.primary.id
    availability_zone = "us-east-2c"
    cidr_block = "10.0.49.0/24"
}

resource "aws_route_table_association" "k8s_private_rt_subnet_a" {
    route_table_id = aws_route_table.private_route_table_a.id
    subnet_id = aws_subnet.k8s_subnet_private_a.id
}

resource "aws_route_table_association" "k8s_private_rt_subnet_c" {
    route_table_id = aws_route_table.private_route_table_c.id
    subnet_id = aws_subnet.k8s_subnet_private_c.id
}

resource "aws_route_table_association" "k8s_public_rt_subnet_a_v2" {
    route_table_id = aws_route_table.public_route_table.id
    subnet_id = aws_subnet.k8s_subnet_public_a_v2.id
}

resource "aws_route_table_association" "k8s_public_rt_subnet_c_v2" {
    route_table_id = aws_route_table.public_route_table.id
    subnet_id = aws_subnet.k8s_subnet_public_c_v2.id
}

resource "aws_route_table_association" "fargate_private_rt_subnet_a" {
    route_table_id = aws_route_table.private_route_table_a.id
    subnet_id = aws_subnet.fargate_subnet_private_a.id
}

resource "aws_route_table_association" "fargate_private_rt_subnet_c" {
    route_table_id = aws_route_table.private_route_table_c.id
    subnet_id = aws_subnet.fargate_subnet_private_c.id
}

resource "aws_route_table_association" "fargate_private_rt_subnet_a_v2" {
    route_table_id = aws_route_table.private_route_table_a.id
    subnet_id = aws_subnet.fargate_subnet_private_a_v2.id
}

resource "aws_route_table_association" "fargate_private_rt_subnet_c_v2" {
    route_table_id = aws_route_table.private_route_table_c.id
    subnet_id = aws_subnet.fargate_subnet_private_c_v2.id
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
        aws_subnet.fargate_subnet_private_c.id,
        aws_subnet.fargate_subnet_private_a_v2.id,
        aws_subnet.fargate_subnet_private_c_v2.id
    ]
}