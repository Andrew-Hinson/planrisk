# Small local module so the fixture includes a nested module address.
variable "name" {
  type = string
}

variable "visibility_timeout" {
  type = number
}

resource "aws_sqs_queue" "this" {
  name                       = var.name
  visibility_timeout_seconds = var.visibility_timeout
}
