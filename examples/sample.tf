# Terraform / HCL sample for rvim syntax highlighting.

terraform {
  required_version = ">= 1.5"
}

variable "bucket_names" {
  type    = list(string)
  default = ["logs", "backups"]
}

/* Create one bucket per name, with a merged tag set. */
resource "aws_s3_bucket" "this" {
  for_each = toset(var.bucket_names)
  bucket   = "example-${each.value}"

  tags = merge(local.common_tags, {
    Name = each.value
  })
}

locals {
  common_tags = {
    Project = "rvim"
    Owner   = "cjjulius"
  }
}

output "bucket_count" {
  value = length(aws_s3_bucket.this) // one per name
}

# Heredocs span lines until a line that is just the terminator tag.
locals {
  policy = <<-EOT
    {
      "Version": "2012-10-17",
      "Statement": []
    }
  EOT
}
