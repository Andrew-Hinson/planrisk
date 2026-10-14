# Realistic destructive changes: stateful replaces and deletes, a rename
# without a moved block, a removed block, and replace_triggered_by.
# Planned against the state built from before/ (see ../../generate.sh).

terraform {
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 6.68"
    }
  }
}

provider "aws" {
  region                      = "us-east-1"
  skip_credentials_validation = true
  skip_requesting_account_id  = true
  skip_metadata_api_check     = true
}
