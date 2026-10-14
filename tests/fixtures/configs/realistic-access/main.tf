# 1. Engine upgrade: an in-place update, not a replace.
resource "aws_db_instance" "main" {
  identifier          = "app"
  engine              = "postgres"
  engine_version      = "16.3"
  instance_class      = "db.t4g.micro"
  allocated_storage   = 20
  username            = "app"
  password            = "not-a-real-password"
  skip_final_snapshot = true
}

# 2. Already open to the world; only a tag changes. Must not fire.
resource "aws_security_group" "web" {
  name        = "web"
  description = "web"
  ingress {
    from_port   = 443
    to_port     = 443
    protocol    = "tcp"
    cidr_blocks = ["0.0.0.0/0"]
  }
  tags = { team = "web" }
}

resource "aws_iam_role" "app" {
  name = "app"
  assume_role_policy = jsonencode({
    Version   = "2012-10-17"
    Statement = [{ Effect = "Allow", Action = "sts:AssumeRole", Principal = { Service = "ec2.amazonaws.com" } }]
  })
}

# 3. Wildcard action on a bucket that doesn't exist yet: the policy is unknown at plan time.
resource "aws_s3_bucket" "new" { bucket_prefix = "new-" }

resource "aws_iam_role_policy" "app" {
  role = "app"
  policy = jsonencode({
    Version   = "2012-10-17"
    Statement = [{ Effect = "Allow", Action = "*", Resource = ["${aws_s3_bucket.new.arn}/*"] }]
  })
}

# 4. Admin policy written with a policy document data source.
data "aws_iam_policy_document" "admin" {
  statement {
    actions   = ["*"]
    resources = ["*"]
  }
}

resource "aws_iam_policy" "admin" {
  name   = "admin"
  policy = data.aws_iam_policy_document.admin.json
}

resource "aws_s3_bucket" "data" { bucket = "data" }

# 5. Public bucket policy on an existing bucket.
resource "aws_s3_bucket_policy" "data" {
  bucket = "data"
  policy = jsonencode({
    Version   = "2012-10-17"
    Statement = [{ Effect = "Allow", Principal = "*", Action = "s3:GetObject", Resource = "arn:aws:s3:::data/*" }]
  })
}

resource "aws_s3_bucket_public_access_block" "data" {
  bucket                  = "data"
  block_public_acls       = true
  block_public_policy     = false
  ignore_public_acls      = true
  restrict_public_buckets = false
}

# 6. count -> for_each refactor with moved blocks: no real change.
resource "aws_instance" "web" {
  for_each      = toset(["a", "b"])
  ami           = "ami-12345678"
  instance_type = "t3.micro"
}

moved {
  from = aws_instance.web[0]
  to   = aws_instance.web["a"]
}

moved {
  from = aws_instance.web[1]
  to   = aws_instance.web["b"]
}

# 7. Module keys with unusual characters, one added.
module "queues" {
  source   = "./modules/queue"
  for_each = toset(["eu-west/1", "us \"east\""])
  name     = "q-${replace(each.key, "/[^a-zA-Z0-9]/", "-")}"
}

# 8. New internet-facing load balancer with internal left unset, and a public SG rule.
resource "aws_lb" "public" {
  name    = "public"
  subnets = ["subnet-12345678", "subnet-87654321"]
}

resource "aws_vpc_security_group_ingress_rule" "ssh" {
  security_group_id = aws_security_group.web.id
  ip_protocol       = "tcp"
  from_port         = 22
  to_port           = 22
  cidr_ipv6         = "::/0"
}
