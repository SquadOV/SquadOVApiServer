resource "aws_iam_role" "eks_role" {
    name = "eks-cluster-role"
    force_detach_policies = true

    assume_role_policy = <<POLICY
{
    "Version": "2012-10-17",
    "Statement": [
        {
            "Effect": "Allow",
            "Principal": {
                "Service": [
                    "eks-fargate-pods.amazonaws.com",
                    "eks.amazonaws.com",
                    "ec2.amazonaws.com"
                ]
            },
            "Action": "sts:AssumeRole"
        }
    ]
}
POLICY
}

resource "aws_iam_role" "eks_managed_role" {
    name = "eks-managed-role"
    force_detach_policies = true

    assume_role_policy = <<POLICY
{
    "Version": "2012-10-17",
    "Statement": [
        {
            "Effect": "Allow",
            "Principal": {
                "Service": [
                    "eks.amazonaws.com",
                    "ec2.amazonaws.com"
                ]
            },
            "Action": "sts:AssumeRole"
        }
    ]
}
POLICY
}

resource "aws_iam_policy" "eks_policy_cloudwatch_logs" {
    name = "eks-policy-cloudwatch-logs"
    description = "EKS Pod Execution Policy to allow sending logs to CloudWatch"
    policy = <<POLICY
{
	"Version": "2012-10-17",
	"Statement": [{
		"Effect": "Allow",
		"Action": [
			"logs:CreateLogStream",
			"logs:CreateLogGroup",
			"logs:DescribeLogStreams",
			"logs:PutLogEvents",
            "ec2:DescribeVolumes"
		],
		"Resource": "*"
	}]
}
POLICY
}

resource "aws_iam_policy" "eks_policy_alb" {
    name = "eks-policy-arb"
    description = "EKS ALB Policy"
    policy = <<POLICY
{
    "Version": "2012-10-17",
    "Statement": [
        {
            "Effect": "Allow",
            "Action": [
                "iam:CreateServiceLinkedRole",
                "ec2:DescribeAccountAttributes",
                "ec2:DescribeAddresses",
                "ec2:DescribeAvailabilityZones",
                "ec2:DescribeInternetGateways",
                "ec2:DescribeVpcs",
                "ec2:DescribeSubnets",
                "ec2:DescribeSecurityGroups",
                "ec2:DescribeInstances",
                "ec2:DescribeNetworkInterfaces",
                "ec2:DescribeTags",
                "ec2:GetCoipPoolUsage",
                "ec2:DescribeCoipPools",
                "elasticloadbalancing:DescribeLoadBalancers",
                "elasticloadbalancing:DescribeLoadBalancerAttributes",
                "elasticloadbalancing:DescribeListeners",
                "elasticloadbalancing:DescribeListenerCertificates",
                "elasticloadbalancing:DescribeSSLPolicies",
                "elasticloadbalancing:DescribeRules",
                "elasticloadbalancing:DescribeTargetGroups",
                "elasticloadbalancing:DescribeTargetGroupAttributes",
                "elasticloadbalancing:DescribeTargetHealth",
                "elasticloadbalancing:DescribeTags"
            ],
            "Resource": "*"
        },
        {
            "Effect": "Allow",
            "Action": [
                "cognito-idp:DescribeUserPoolClient",
                "acm:ListCertificates",
                "acm:DescribeCertificate",
                "iam:ListServerCertificates",
                "iam:GetServerCertificate",
                "waf-regional:GetWebACL",
                "waf-regional:GetWebACLForResource",
                "waf-regional:AssociateWebACL",
                "waf-regional:DisassociateWebACL",
                "wafv2:GetWebACL",
                "wafv2:GetWebACLForResource",
                "wafv2:AssociateWebACL",
                "wafv2:DisassociateWebACL",
                "shield:GetSubscriptionState",
                "shield:DescribeProtection",
                "shield:CreateProtection",
                "shield:DeleteProtection"
            ],
            "Resource": "*"
        },
        {
            "Effect": "Allow",
            "Action": [
                "ec2:AuthorizeSecurityGroupIngress",
                "ec2:RevokeSecurityGroupIngress"
            ],
            "Resource": "*"
        },
        {
            "Effect": "Allow",
            "Action": [
                "ec2:CreateSecurityGroup"
            ],
            "Resource": "*"
        },
        {
            "Effect": "Allow",
            "Action": [
                "ec2:CreateTags"
            ],
            "Resource": "arn:aws:ec2:*:*:security-group/*",
            "Condition": {
                "StringEquals": {
                    "ec2:CreateAction": "CreateSecurityGroup"
                },
                "Null": {
                    "aws:RequestTag/elbv2.k8s.aws/cluster": "false"
                }
            }
        },
        {
            "Effect": "Allow",
            "Action": [
                "ec2:CreateTags",
                "ec2:DeleteTags"
            ],
            "Resource": "arn:aws:ec2:*:*:security-group/*",
            "Condition": {
                "Null": {
                    "aws:RequestTag/elbv2.k8s.aws/cluster": "true",
                    "aws:ResourceTag/elbv2.k8s.aws/cluster": "false"
                }
            }
        },
        {
            "Effect": "Allow",
            "Action": [
                "ec2:AuthorizeSecurityGroupIngress",
                "ec2:RevokeSecurityGroupIngress",
                "ec2:DeleteSecurityGroup"
            ],
            "Resource": "*",
            "Condition": {
                "Null": {
                    "aws:ResourceTag/elbv2.k8s.aws/cluster": "false"
                }
            }
        },
        {
            "Effect": "Allow",
            "Action": [
                "elasticloadbalancing:CreateLoadBalancer",
                "elasticloadbalancing:CreateTargetGroup"
            ],
            "Resource": "*",
            "Condition": {
                "Null": {
                    "aws:RequestTag/elbv2.k8s.aws/cluster": "false"
                }
            }
        },
        {
            "Effect": "Allow",
            "Action": [
                "elasticloadbalancing:CreateListener",
                "elasticloadbalancing:DeleteListener",
                "elasticloadbalancing:CreateRule",
                "elasticloadbalancing:DeleteRule"
            ],
            "Resource": "*"
        },
        {
            "Effect": "Allow",
            "Action": [
                "elasticloadbalancing:AddTags",
                "elasticloadbalancing:RemoveTags"
            ],
            "Resource": [
                "arn:aws:elasticloadbalancing:*:*:targetgroup/*/*",
                "arn:aws:elasticloadbalancing:*:*:loadbalancer/net/*/*",
                "arn:aws:elasticloadbalancing:*:*:loadbalancer/app/*/*"
            ],
            "Condition": {
                "Null": {
                    "aws:RequestTag/elbv2.k8s.aws/cluster": "true",
                    "aws:ResourceTag/elbv2.k8s.aws/cluster": "false"
                }
            }
        },
        {
            "Effect": "Allow",
            "Action": [
                "elasticloadbalancing:AddTags",
                "elasticloadbalancing:RemoveTags"
            ],
            "Resource": [
                "arn:aws:elasticloadbalancing:*:*:listener/net/*/*/*",
                "arn:aws:elasticloadbalancing:*:*:listener/app/*/*/*",
                "arn:aws:elasticloadbalancing:*:*:listener-rule/net/*/*/*",
                "arn:aws:elasticloadbalancing:*:*:listener-rule/app/*/*/*"
            ]
        },
        {
            "Effect": "Allow",
            "Action": [
                "elasticloadbalancing:ModifyLoadBalancerAttributes",
                "elasticloadbalancing:SetIpAddressType",
                "elasticloadbalancing:SetSecurityGroups",
                "elasticloadbalancing:SetSubnets",
                "elasticloadbalancing:DeleteLoadBalancer",
                "elasticloadbalancing:ModifyTargetGroup",
                "elasticloadbalancing:ModifyTargetGroupAttributes",
                "elasticloadbalancing:DeleteTargetGroup"
            ],
            "Resource": "*",
            "Condition": {
                "Null": {
                    "aws:ResourceTag/elbv2.k8s.aws/cluster": "false"
                }
            }
        },
        {
            "Effect": "Allow",
            "Action": [
                "elasticloadbalancing:RegisterTargets",
                "elasticloadbalancing:DeregisterTargets"
            ],
            "Resource": "arn:aws:elasticloadbalancing:*:*:targetgroup/*/*"
        },
        {
            "Effect": "Allow",
            "Action": [
                "elasticloadbalancing:SetWebAcl",
                "elasticloadbalancing:ModifyListener",
                "elasticloadbalancing:AddListenerCertificates",
                "elasticloadbalancing:RemoveListenerCertificates",
                "elasticloadbalancing:ModifyRule"
            ],
            "Resource": "*"
        }
    ]
}
POLICY
}

resource "aws_iam_role_policy_attachment" "eks_role_cloudwatch_policy" {
    policy_arn = aws_iam_policy.eks_policy_cloudwatch_logs.arn
    role       = aws_iam_role.eks_role.name
}

resource "aws_iam_role_policy_attachment" "eks_role_AmazonEKSClusterPolicy" {
    policy_arn = "arn:aws:iam::aws:policy/AmazonEKSClusterPolicy"
    role       = aws_iam_role.eks_role.name
}

# Optionally, enable Security Groups for Pods
# Reference: https://docs.aws.amazon.com/eks/latest/userguide/security-groups-for-pods.html
resource "aws_iam_role_policy_attachment" "eks_role_AmazonEKSVPCResourceController" {
    policy_arn = "arn:aws:iam::aws:policy/AmazonEKSVPCResourceController"
    role       = aws_iam_role.eks_role.name
}

resource "aws_iam_role_policy_attachment" "eks_role_AmazonEKSFargatePodExecutionRolePolicy" {
    policy_arn = "arn:aws:iam::aws:policy/AmazonEKSFargatePodExecutionRolePolicy"
    role       = aws_iam_role.eks_role.name
}

resource "aws_iam_role_policy_attachment" "eks_role_AmazonEKS_CNI_Policy" {
    policy_arn = "arn:aws:iam::aws:policy/AmazonEKS_CNI_Policy"
    role       = aws_iam_role.eks_role.name
}

resource "aws_iam_role_policy_attachment" "eks_managed_AmazonEKS_CNI_Policy" {
    policy_arn = "arn:aws:iam::aws:policy/AmazonEKS_CNI_Policy"
    role       = aws_iam_role.eks_managed_role.name
}

resource "aws_iam_role_policy_attachment" "eks_managed_AmazonEKSWorkerNodePolicy" {
    policy_arn = "arn:aws:iam::aws:policy/AmazonEKSWorkerNodePolicy"
    role       = aws_iam_role.eks_managed_role.name
}

resource "aws_iam_role_policy_attachment" "eks_managed_AmazonEC2ContainerRegistryReadOnly" {
    policy_arn = "arn:aws:iam::aws:policy/AmazonEC2ContainerRegistryReadOnly"
    role       = aws_iam_role.eks_managed_role.name
}

resource "aws_iam_role_policy_attachment" "eks_managed_cloudwatch_policy" {
    policy_arn = aws_iam_policy.eks_policy_cloudwatch_logs.arn
    role       = aws_iam_role.eks_managed_role.name
}

resource "aws_iam_role_policy_attachment" "eks_managed_CloudWatchAgentServerPolicy" {
    policy_arn = "arn:aws:iam::aws:policy/CloudWatchAgentServerPolicy"
    role       = aws_iam_role.eks_managed_role.name
}

resource "aws_kms_key" "primary_eks_kms_key" {
    description = "KMS Key: Primary EKS Cluster"
    customer_master_key_spec = "SYMMETRIC_DEFAULT"
}

resource "aws_eks_cluster" "primary_eks_cluster" {
    name = "primary-eks-cluster"
    role_arn = aws_iam_role.eks_role.arn

    vpc_config {
        subnet_ids = concat(var.public_k8s_subnets, var.private_k8s_subnets)
    }

    enabled_cluster_log_types = ["api", "audit", "scheduler"]
    encryption_config {
        provider {
            key_arn = aws_kms_key.primary_eks_kms_key.arn
        }

        resources = ["secrets"]
    }

    version = "1.20"

    depends_on = [
        aws_iam_role_policy_attachment.eks_role_AmazonEKSClusterPolicy,
        aws_iam_role_policy_attachment.eks_role_AmazonEKSVPCResourceController,
    ]
}

resource "aws_eks_addon" "primary_eks_vpccni_addon" {
    cluster_name = aws_eks_cluster.primary_eks_cluster.name
    addon_name = "vpc-cni"
}

data "tls_certificate" "primary_eks_tls_certificate" {
    url = aws_eks_cluster.primary_eks_cluster.identity[0].oidc[0].issuer
}

resource "aws_iam_openid_connect_provider" "primary_eks_iam_provider" {
    url = aws_eks_cluster.primary_eks_cluster.identity[0].oidc[0].issuer

    client_id_list = [
        "sts.amazonaws.com"
    ]

    thumbprint_list = [
        data.tls_certificate.primary_eks_tls_certificate.certificates[0].sha1_fingerprint
    ]
}

resource "aws_eks_fargate_profile" "primary_eks_default_fargate_profile" {
    cluster_name = aws_eks_cluster.primary_eks_cluster.name
    fargate_profile_name = "default-fargate-profile"
    pod_execution_role_arn = aws_iam_role.eks_role.arn
    subnet_ids = var.default_fargate_subnets

    selector {
        namespace = "default"
    }
}

resource "aws_eks_fargate_profile" "primary_eks_cert_manager_fargate_profile" {
    cluster_name = aws_eks_cluster.primary_eks_cluster.name
    fargate_profile_name = "cert-manager-fargate-profile"
    pod_execution_role_arn = aws_iam_role.eks_role.arn
    subnet_ids = var.default_fargate_subnets

    selector {
        namespace = "cert-manager"
    }
}

resource "aws_eks_fargate_profile" "primary_eks_kube_system_fargate_profile" {
    cluster_name = aws_eks_cluster.primary_eks_cluster.name
    fargate_profile_name = "kube-system-fargate-profile"
    pod_execution_role_arn = aws_iam_role.eks_role.arn
    subnet_ids = var.default_fargate_subnets

    selector {
        namespace = "kube-system"
        labels = {
            "k8s-app" = "kube-dns"
        }
    }
}

resource "aws_eks_fargate_profile" "primary_eks_alb_fargate_profile" {
    cluster_name = aws_eks_cluster.primary_eks_cluster.name
    fargate_profile_name = "alb-ingress-fargate-profile"
    pod_execution_role_arn = aws_iam_role.eks_role.arn
    subnet_ids = var.default_fargate_subnets

    selector {
        namespace = "kube-system"
        labels = {
            "app.kubernetes.io/name" = "alb-ingress-controller"
        }
    }
}

resource "aws_eks_node_group" "system_nodes" {
    cluster_name = aws_eks_cluster.primary_eks_cluster.name
    node_group_name = "primary-eks-system-nodes"
    node_role_arn = aws_iam_role.eks_managed_role.arn
    subnet_ids = var.default_fargate_subnets

    capacity_type = "ON_DEMAND"
    instance_types = [ "t3.small" ]

    labels = {
        "task" = "system"
        "provision" = "manual"
    }

    scaling_config {
        desired_size = 1
        min_size = 1
        max_size = 1
    }

    depends_on = [
        aws_iam_role_policy_attachment.eks_managed_AmazonEKSWorkerNodePolicy,
        aws_iam_role_policy_attachment.eks_managed_AmazonEKS_CNI_Policy,
        aws_iam_role_policy_attachment.eks_managed_AmazonEC2ContainerRegistryReadOnly
    ]

    lifecycle {
        ignore_changes = [scaling_config[0].desired_size]
    }
}

resource "aws_eks_node_group" "vod_nodes_core" {
    cluster_name = aws_eks_cluster.primary_eks_cluster.name
    node_group_name = "primary-eks-vod-nodes-core"
    node_role_arn = aws_iam_role.eks_managed_role.arn
    subnet_ids = var.default_fargate_subnets

    capacity_type = "ON_DEMAND"
    disk_size = 50
    instance_types = [ "m5.xlarge" ]

    labels = {
        "task" = "vod"
        "capacity" = "demand"
        "provision" = "manual"
    }

    scaling_config {
        desired_size = 1
        min_size = 1
        max_size = 5
    }

    depends_on = [
        aws_iam_role_policy_attachment.eks_managed_AmazonEKSWorkerNodePolicy,
        aws_iam_role_policy_attachment.eks_managed_AmazonEKS_CNI_Policy,
        aws_iam_role_policy_attachment.eks_managed_AmazonEC2ContainerRegistryReadOnly
    ]

    lifecycle {
        ignore_changes = [scaling_config[0].desired_size]
    }
}

resource "aws_eks_node_group" "vod_nodes_secondary" {
    cluster_name = aws_eks_cluster.primary_eks_cluster.name
    node_group_name = "primary-eks-vod-nodes-secondary"
    node_role_arn = aws_iam_role.eks_managed_role.arn
    subnet_ids = var.default_fargate_subnets

    capacity_type = "SPOT"
    disk_size = 50
    instance_types = [ "m5.xlarge" ]

    labels = {
        "task" = "vod"
        "capacity" = "spot"
        "provision" = "manual"
    }

    scaling_config {
        desired_size = 1
        min_size = 1
        max_size = 24
    }

    depends_on = [
        aws_iam_role_policy_attachment.eks_managed_AmazonEKSWorkerNodePolicy,
        aws_iam_role_policy_attachment.eks_managed_AmazonEKS_CNI_Policy,
        aws_iam_role_policy_attachment.eks_managed_AmazonEC2ContainerRegistryReadOnly
    ]

    lifecycle {
        ignore_changes = [scaling_config[0].desired_size]
    }
}

resource "aws_iam_policy" "eks_policy_autoscaler" {
    name = "eks-policy-autoscaler"
    description = "EKS Policy for Cluster Autoscaler"
    policy = <<POLICY
{
    "Version": "2012-10-17",
    "Statement": [
        {
            "Action": [
                "autoscaling:DescribeAutoScalingGroups",
                "autoscaling:DescribeAutoScalingInstances",
                "autoscaling:DescribeLaunchConfigurations",
                "autoscaling:DescribeTags",
                "autoscaling:SetDesiredCapacity",
                "autoscaling:TerminateInstanceInAutoScalingGroup",
                "ec2:DescribeLaunchTemplateVersions"
            ],
            "Resource": "*",
            "Effect": "Allow"
        }
    ]
}
POLICY
}

resource "aws_eks_node_group" "prometheus_nodes" {
    cluster_name = aws_eks_cluster.primary_eks_cluster.name
    node_group_name = "primary-eks-vod-nodes-secondary"
    node_role_arn = aws_iam_role.eks_managed_role.arn
    subnet_ids = var.default_fargate_subnets

    capacity_type = "ON_DEMAND"
    instance_types = [ "m5.xlarge" ]

    labels = {
        "task" = "prometheus"
    }

    scaling_config {
        desired_size = 1
        min_size = 1
        max_size = 1
    }

    depends_on = [
        aws_iam_role_policy_attachment.eks_managed_AmazonEKSWorkerNodePolicy,
        aws_iam_role_policy_attachment.eks_managed_AmazonEKS_CNI_Policy,
        aws_iam_role_policy_attachment.eks_managed_AmazonEC2ContainerRegistryReadOnly
    ]

    lifecycle {
        ignore_changes = [scaling_config[0].desired_size]
    }
}