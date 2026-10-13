# Changes that widen network access, and some that look similar but do not.
terraform {
  required_version = ">= 1.0"
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 6.68"
    }
  }
}

provider "aws" {
  region = "us-east-1"

  # Never calls AWS: creates need no API calls, and the plan runs with
  # -refresh=false against a seeded state (see ../../seed_state.py).
  skip_credentials_validation = true
  skip_requesting_account_id  = true
  skip_metadata_api_check     = true
}

# Gains SSH from anywhere (IPv4) next to its existing private rule: fires.
resource "aws_security_group" "web" {
  name        = "planrisk-fixture-web"
  description = "Web servers"

  ingress {
    description = "HTTPS from the VPC"
    from_port   = 443
    to_port     = 443
    protocol    = "tcp"
    cidr_blocks = ["10.0.0.0/8"]
  }

  ingress {
    description = "SSH from anywhere"
    from_port   = 22
    to_port     = 22
    protocol    = "tcp"
    cidr_blocks = ["0.0.0.0/0"]
  }
}

# Unchanged, already open: does not fire.
resource "aws_security_group" "public_https" {
  name        = "planrisk-fixture-public-https"
  description = "Public HTTPS"

  ingress {
    description = "HTTPS from anywhere"
    from_port   = 443
    to_port     = 443
    protocol    = "tcp"
    cidr_blocks = ["0.0.0.0/0"]
  }
}

# Older standalone rule resource, IPv6 anywhere: fires.
resource "aws_security_group_rule" "ssh_ipv6" {
  type              = "ingress"
  security_group_id = aws_security_group.web.id
  from_port         = 22
  to_port           = 22
  protocol          = "tcp"
  ipv6_cidr_blocks  = ["::/0"]
}

# Egress to anywhere is normal: does not fire.
resource "aws_security_group_rule" "egress_all" {
  type              = "egress"
  security_group_id = aws_security_group.web.id
  from_port         = 0
  to_port           = 0
  protocol          = "-1"
  cidr_blocks       = ["0.0.0.0/0"]
}

# Newer rule resource, every port and protocol from anywhere: fires.
resource "aws_vpc_security_group_ingress_rule" "everything" {
  security_group_id = aws_security_group.web.id
  ip_protocol       = "-1"
  cidr_ipv4         = "0.0.0.0/0"
}

# Newer rule resource, private range only: does not fire.
resource "aws_vpc_security_group_ingress_rule" "vpc_postgres" {
  security_group_id = aws_security_group.web.id
  ip_protocol       = "tcp"
  from_port         = 5432
  to_port           = 5432
  cidr_ipv4         = "10.0.0.0/16"
}

# Widened to full "*:*" administrative privileges: fires.
resource "aws_iam_policy" "reader" {
  name = "planrisk-fixture-reader"
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Sid      = "Everything"
      Effect   = "Allow"
      Action   = "*"
      Resource = "*"
    }]
  })
}

# Unchanged, already "Resource": "*": does not fire.
resource "aws_iam_policy" "describe" {
  name = "planrisk-fixture-describe"
  policy = jsonencode({
    Version   = "2012-10-17"
    Statement = [{ Effect = "Allow", Action = "ec2:Describe*", Resource = "*" }]
  })
}

# Any AWS account may assume the role, limited by a condition: fires, and
# the finding notes the condition.
resource "aws_iam_role" "app" {
  name = "planrisk-fixture-app"
  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect    = "Allow"
      Action    = "sts:AssumeRole"
      Principal = { AWS = "*" }
      Condition = { StringEquals = { "sts:ExternalId" = "planrisk-fixture" } }
    }]
  })
}

# New inline policy with a specific action on every resource: fires.
resource "aws_iam_role_policy" "app_logs" {
  name = "planrisk-fixture-app-logs"
  role = aws_iam_role.app.id
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect   = "Allow"
      Action   = ["logs:CreateLogStream", "logs:PutLogEvents"]
      Resource = "*"
    }]
  })
}

# Public read for everyone ("Principal": "*"): fires.
resource "aws_s3_bucket_policy" "assets_public" {
  bucket = "planrisk-fixture-assets"
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect    = "Allow"
      Principal = "*"
      Action    = "s3:GetObject"
      Resource  = "arn:aws:s3:::planrisk-fixture-assets/*"
    }]
  })
}

# Deny statements with wildcards restrict access: does not fire.
resource "aws_iam_policy" "deny_all" {
  name = "planrisk-fixture-deny-all"
  policy = jsonencode({
    Version   = "2012-10-17"
    Statement = [{ Effect = "Deny", Action = "*", Resource = "*" }]
  })
}

# Two protections turned off so the public bucket policy above can work: fires.
resource "aws_s3_bucket_public_access_block" "assets" {
  bucket                  = "planrisk-fixture-assets"
  block_public_acls       = true
  block_public_policy     = false
  ignore_public_acls      = true
  restrict_public_buckets = false
}

# aws_s3_bucket_public_access_block.logs is gone from config: its protections
# are removed with it. Fires.

# Unchanged account-wide settings: does not fire.
resource "aws_s3_account_public_access_block" "this" {
  block_public_acls       = true
  block_public_policy     = true
  ignore_public_acls      = true
  restrict_public_buckets = true
}

# New, fully locked-down bucket: does not fire.
resource "aws_s3_bucket_public_access_block" "reports" {
  bucket                  = "planrisk-fixture-reports"
  block_public_acls       = true
  block_public_policy     = true
  ignore_public_acls      = true
  restrict_public_buckets = true
}
