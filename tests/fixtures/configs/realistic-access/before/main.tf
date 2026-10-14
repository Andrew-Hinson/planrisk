resource "aws_db_instance" "main" {
  identifier          = "app"
  engine              = "postgres"
  engine_version      = "15.7"
  instance_class      = "db.t4g.micro"
  allocated_storage   = 20
  username            = "app"
  password            = "not-a-real-password"
  skip_final_snapshot = true
}

resource "aws_security_group" "web" {
  name        = "web"
  description = "web"
  ingress {
    from_port   = 443
    to_port     = 443
    protocol    = "tcp"
    cidr_blocks = ["0.0.0.0/0"]
  }
}

resource "aws_iam_role" "app" {
  name = "app"
  assume_role_policy = jsonencode({
    Version   = "2012-10-17"
    Statement = [{ Effect = "Allow", Action = "sts:AssumeRole", Principal = { Service = "ec2.amazonaws.com" } }]
  })
}

resource "aws_iam_role_policy" "app" {
  role = "app"
  policy = jsonencode({
    Version   = "2012-10-17"
    Statement = [{ Effect = "Allow", Action = ["s3:GetObject"], Resource = ["arn:aws:s3:::data/*"] }]
  })
}

resource "aws_s3_bucket" "data" { bucket = "data" }

resource "aws_s3_bucket_public_access_block" "data" {
  bucket                  = "data"
  block_public_acls       = true
  block_public_policy     = true
  ignore_public_acls      = true
  restrict_public_buckets = true
}

resource "aws_instance" "web" {
  count         = 2
  ami           = "ami-12345678"
  instance_type = "t3.micro"
}

module "queues" {
  source   = "./modules/queue"
  for_each = toset(["eu-west/1"])
  name     = "q-${replace(each.key, "/", "-")}"
}
