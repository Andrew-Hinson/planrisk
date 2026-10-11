# 40 queues updated and 6 disk volumes removed at once: a large blast radius.
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

# Every queue's timeout changes: 40 in-place updates.
resource "aws_sqs_queue" "jobs" {
  count                      = 40
  name                       = "planrisk-fixture-job-${count.index}"
  visibility_timeout_seconds = 60
}

# aws_ebs_volume.data is gone from config: 6 stateful deletes.
