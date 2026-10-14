# A realistic mix of access changes, with cases that must not fire: an
# already-open security group, a moved-block refactor, an engine upgrade.
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
