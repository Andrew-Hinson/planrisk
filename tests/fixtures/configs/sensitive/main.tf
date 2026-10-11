# Secrets flowing into resources, to test that planrisk never prints them.
# `terraform show -json` writes sensitive values in plain text, so the
# fixture JSON contains both secrets below; planrisk's output must not.
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

# Marked sensitive in configuration.
variable "db_password" {
  type      = string
  sensitive = true
  default   = "PLANRISK-FIXTURE-SECRET-VARIABLE"
}

resource "aws_db_instance" "main" {
  identifier          = "planrisk-fixture-db"
  engine              = "postgres"
  instance_class      = "db.t4g.micro"
  allocated_storage   = 20
  username            = "planrisk"
  password            = var.db_password
  skip_final_snapshot = true
}

resource "aws_secretsmanager_secret" "api" {
  name = "planrisk-fixture-api-key"
}

# `secret_string` is marked sensitive by the provider's schema,
# not by this configuration.
resource "aws_secretsmanager_secret_version" "api" {
  secret_id     = aws_secretsmanager_secret.api.id
  secret_string = "PLANRISK-FIXTURE-SECRET-SCHEMA"
}
