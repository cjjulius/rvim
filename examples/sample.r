# R sample for rvim syntax highlighting.

# Euclidean distance between two points given as numeric vectors.
distance <- function(a, b) {
  stopifnot(length(a) == length(b))
  sqrt(sum((a - b)^2))
}

points <- list(c(0, 0), c(3, 4))
origin <- c(0, 0)

for (p in points) {
  d <- distance(p, origin)
  cat(sprintf("distance from origin: %.2f\n", d))
}

# Summaries use built-in vectorized helpers.
values <- rnorm(10, mean = 0, sd = 1)
print(summary(values))
