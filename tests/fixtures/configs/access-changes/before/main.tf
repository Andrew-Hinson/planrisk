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

resource "aws_s3_bucket_public_access_block" "assets" {
  bucket                  = "planrisk-fixture-assets"
  block_public_acls       = true
  block_public_policy     = true
  ignore_public_acls      = true
  restrict_public_buckets = true
}

resource "aws_s3_bucket_public_access_block" "logs" {
  bucket                  = "planrisk-fixture-logs"
  block_public_acls       = true
  block_public_policy     = true
  ignore_public_acls      = true
  restrict_public_buckets = true
}

resource "aws_s3_account_public_access_block" "this" {
  block_public_acls       = true
  block_public_policy     = true
  ignore_public_acls      = true
  restrict_public_buckets = true
}

resource "aws_db_instance" "reporting" {
  identifier          = "planrisk-fixture-reporting"
  engine              = "postgres"
  instance_class      = "db.t4g.micro"
  allocated_storage   = 20
  username            = "planrisk"
  password            = "not-a-real-password"
  publicly_accessible = false
  skip_final_snapshot = true
}

resource "aws_lb" "internal_api" {
  name     = "planrisk-fixture-internal-api"
  internal = true
  subnets  = ["subnet-00000000000000001", "subnet-00000000000000002"]
}

resource "aws_instance" "bastion" {
  ami                         = "ami-00000000000000000"
  instance_type               = "t3.micro"
  associate_public_ip_address = false
}
