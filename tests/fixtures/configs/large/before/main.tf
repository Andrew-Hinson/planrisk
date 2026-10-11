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

resource "aws_sqs_queue" "jobs" {
  count                      = 40
  name                       = "planrisk-fixture-job-${count.index}"
  visibility_timeout_seconds = 30
}

resource "aws_ebs_volume" "data" {
  count             = 6
  availability_zone = "us-east-1a"
  size              = 10
}
