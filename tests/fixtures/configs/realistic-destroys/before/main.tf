resource "aws_db_instance" "orders" {
  identifier          = "orders"
  engine              = "postgres"
  instance_class      = "db.t4g.micro"
  allocated_storage   = 20
  username            = "app"
  password            = "not-a-real-password"
  skip_final_snapshot = true
}

resource "aws_dynamodb_table" "sessions" {
  name         = "sessions"
  billing_mode = "PAY_PER_REQUEST"
  hash_key     = "id"
  attribute {
    name = "id"
    type = "S"
  }
}

resource "aws_s3_bucket" "uploads" { bucket = "uploads" }

resource "aws_ebs_volume" "cache" {
  availability_zone = "us-east-1a"
  size              = 10
}

resource "aws_kms_key" "main" { description = "main" }

resource "aws_instance" "worker" {
  ami           = "ami-12345678"
  instance_type = "t3.micro"
}

resource "aws_sqs_queue" "events" { name = "events" }

resource "aws_sqs_queue" "dlq" { name = "dlq" }
