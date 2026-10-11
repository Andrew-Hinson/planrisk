# Import blocks, using the random provider: it imports from the value alone,
# so no cloud account is needed. (Importing AWS resources reads them from AWS.)
terraform {
  required_version = ">= 1.5" # import blocks need 1.5
  required_providers {
    random = {
      source  = "hashicorp/random"
      version = "~> 3.9"
    }
  }
}

# Config matches the imported value: a pure import, nothing changes.
import {
  to = random_string.matching
  id = "abc123"
}

resource "random_string" "matching" {
  length = 6
}

# Config doesn't match the imported value: Terraform imports it and then
# must replace it ("Warning: this will destroy the imported resource").
import {
  to = random_string.mismatched
  id = "xyz789"
}

resource "random_string" "mismatched" {
  length  = 12
  special = false
}
