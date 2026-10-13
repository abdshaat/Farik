resource "aws_cloudwatch_event_rule" "nightly" {
  name                = "nightly"
  schedule_expression = "cron(0 3 * * ? *)"
}

resource "aws_scheduler_schedule" "hourly" {
  flexible_time_window {
    mode = "OFF"
  }
  schedule_expression = "rate(1 hour)"
}

resource "aws_cloudwatch_event_rule" "from_variable" {
  schedule_expression = "${var.schedule}"
}

resource "aws_cloudwatch_event_rule" "retired" {
  # schedule_expression = "rate(5 minutes)"
}
