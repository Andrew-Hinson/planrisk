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
