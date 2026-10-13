# The resources as they "already exist". generate.sh plans this first and
# turns the result into the state that ../main.tf is planned against.
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
}

# Already open to the internet before this change: must not be reported again.
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

resource "aws_iam_policy" "reader" {
  name = "planrisk-fixture-reader"
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect   = "Allow"
      Action   = ["s3:GetObject"]
      Resource = "arn:aws:s3:::planrisk-fixture-assets/*"
    }]
  })
}

# Already uses "Resource": "*" before this change: must not be reported again.
resource "aws_iam_policy" "describe" {
  name = "planrisk-fixture-describe"
  policy = jsonencode({
    Version   = "2012-10-17"
    Statement = [{ Effect = "Allow", Action = "ec2:Describe*", Resource = "*" }]
  })
}

resource "aws_iam_role" "app" {
  name = "planrisk-fixture-app"
  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect    = "Allow"
      Action    = "sts:AssumeRole"
      Principal = { Service = "ec2.amazonaws.com" }
    }]
  })
}
