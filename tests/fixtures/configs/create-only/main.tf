# A few AWS resources that are all created from scratch.
# Planned with fake credentials, so no AWS account is needed (see ../../README.md).
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

  # Planning a create never calls AWS, so skip every check that would.
  skip_credentials_validation = true
  skip_requesting_account_id  = true
  skip_metadata_api_check     = true
}

resource "aws_s3_bucket" "assets" {
  bucket = "planrisk-fixture-assets"
}

resource "aws_security_group" "web" {
  name        = "planrisk-fixture-web"
  description = "Fixture security group"

  ingress {
    description = "HTTPS"
    from_port   = 443
    to_port     = 443
    protocol    = "tcp"
    cidr_blocks = ["10.0.0.0/8"]
  }
}

module "queue" {
  source = "./modules/queue"
  name   = "planrisk-fixture-jobs"
}
