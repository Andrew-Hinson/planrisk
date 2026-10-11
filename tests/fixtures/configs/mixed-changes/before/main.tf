# The resources as they "already exist". generate.sh plans this first and
# turns the result into the state that ../main.tf is planned against.
terraform {
  required_version = ">= 1.7" # removed blocks need 1.7
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

resource "aws_s3_bucket" "logs" {
  bucket = "planrisk-fixture-logs"
}

resource "aws_db_instance" "main" {
  identifier          = "planrisk-fixture-db"
  engine              = "postgres"
  instance_class      = "db.t4g.micro"
  allocated_storage   = 20
  username            = "planrisk"
  password            = "not-a-real-password"
  availability_zone   = "us-east-1a"
  skip_final_snapshot = true
}

resource "aws_security_group" "web" {
  name        = "planrisk-fixture-web"
  description = "Web servers"
  lifecycle {
    create_before_destroy = true
  }
}

resource "aws_sqs_queue" "jobs" {
  name                       = "planrisk-fixture-jobs"
  visibility_timeout_seconds = 30
}

resource "aws_sqs_queue" "unused" {
  name = "planrisk-fixture-unused"
}

resource "aws_sqs_queue" "old_name" {
  name = "planrisk-fixture-moved"
}

resource "aws_sqs_queue" "legacy" {
  name = "planrisk-fixture-legacy"
}

resource "aws_sqs_queue" "workers" {
  count = 3
  name  = "planrisk-fixture-worker-${count.index}"
}

module "queue" {
  source             = "../modules/queue"
  name               = "planrisk-fixture-module-queue"
  visibility_timeout = 30
}
