# One of each kind of change, planned against the state built from before/.
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

# aws_s3_bucket.logs is gone from config: stateful delete.

# Availability zone can't change in place: stateful replace.
resource "aws_db_instance" "main" {
  identifier          = "planrisk-fixture-db"
  engine              = "postgres"
  instance_class      = "db.t4g.micro"
  allocated_storage   = 20
  username            = "planrisk"
  password            = "not-a-real-password"
  availability_zone   = "us-east-1b"
  skip_final_snapshot = true
}

# Description can't change in place: create-before-destroy replace.
resource "aws_security_group" "web" {
  name        = "planrisk-fixture-web"
  description = "Web and API servers"
  lifecycle {
    create_before_destroy = true
  }
}

# In-place update.
resource "aws_sqs_queue" "jobs" {
  name                       = "planrisk-fixture-jobs"
  visibility_timeout_seconds = 60
}

# aws_sqs_queue.unused is gone from config: plain delete.

# Renamed in config: a move, not a delete and create.
moved {
  from = aws_sqs_queue.old_name
  to   = aws_sqs_queue.new_name
}

resource "aws_sqs_queue" "new_name" {
  name = "planrisk-fixture-moved"
}

# No longer managed, but not destroyed: forget.
removed {
  from = aws_sqs_queue.legacy
  lifecycle {
    destroy = false
  }
}

# Count lowered from 3: workers[2] is deleted.
resource "aws_sqs_queue" "workers" {
  count = 2
  name  = "planrisk-fixture-worker-${count.index}"
}

# In-place update inside a module.
module "queue" {
  source             = "./modules/queue"
  name               = "planrisk-fixture-module-queue"
  visibility_timeout = 60
}
