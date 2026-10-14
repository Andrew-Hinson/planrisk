# Turning on encryption forces a replace of a database.
resource "aws_db_instance" "orders" {
  identifier          = "orders"
  storage_encrypted   = true
  engine              = "postgres"
  instance_class      = "db.t4g.micro"
  allocated_storage   = 20
  username            = "app"
  password            = "not-a-real-password"
  skip_final_snapshot = true
}

# Renamed in config without a moved block: destroy + create.
resource "aws_dynamodb_table" "user_sessions" {
  name         = "sessions"
  billing_mode = "PAY_PER_REQUEST"
  hash_key     = "id"
  attribute {
    name = "id"
    type = "S"
  }
}

# Removed from Terraform but kept in AWS: a forget, not a delete.
removed {
  from = aws_s3_bucket.uploads
  lifecycle { destroy = false }
}

# aws_ebs_volume.cache deleted from config: destroyed.

# create_before_destroy replace of a stateful resource.
resource "aws_kms_key" "main" {
  description = "main key"
  key_usage   = "SIGN_VERIFY"
  customer_master_key_spec = "RSA_2048"
  lifecycle { create_before_destroy = true }
}

# AMI change: replace of a non-stateful resource.
resource "aws_instance" "worker" {
  ami           = "ami-87654321"
  instance_type = "t3.micro"
}

resource "aws_sqs_queue" "events" { name = "events" }


resource "aws_sqs_queue" "dlq" {
  name = "dlq"
  lifecycle { replace_triggered_by = [aws_instance.worker] }
}
