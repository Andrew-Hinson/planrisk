# Small local module so the fixture includes a nested module address.
variable "name" {
  type = string
}

resource "aws_sqs_queue" "this" {
  name = var.name
}
