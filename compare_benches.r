#!/usr/bin/Rscript

# Prints a formatted Markdown table of measurement results, and improvement over
# a previous measurement. To be used together with the bench_decode example.
# Columns in the input data should be p10, p50, p90, average, and throughput.

# Usage:
#
#  ./stats.r baseline.dat new.dat
#  ./stats.r baseline.dat new.dat

prev <- read.table(commandArgs(trailingOnly = TRUE)[1])
data <- read.table(commandArgs(trailingOnly = TRUE)[2])

# Note: `sapply` is like `Map` with arguments reversed,
# but it also produces a different data structure as a
# result. Sapply happens to be the convenient thing to
# use here.
dataMean <- sapply(data, mean)
prevMean <- sapply(prev, mean)
dataSd   <- sapply(data, sd)
prevSd   <- sapply(prev, sd)

# Estimates the absolute error in (a ± aErr) / (b ± bErr).
# See also https://en.wikipedia.org/wiki/Propagation_of_uncertainty.
propErr <- function(a, b, aErr, bErr)
{
  # The new relative variance is the sum of the relative variances of a and b.
  relA2 <- (aErr / a) * (aErr / a)
  relB2 <- (bErr / b) * (bErr / b)
  return(abs(b / a) * sqrt(relA2 + relB2))
}

frac <- dataMean / prevMean
fracSd <- propErr(prevMean, dataMean, prevSd, dataSd)

# Make a format string for GH-flavored Markdown.
makeFormat <- function(label, unit)
{
  return(paste('|', label, '| %5.1f ± %3.1f', unit, '| %.3f ± %.3f |\n'))
}

# Mu is for mean, tau for throughput.
cat(sprintf(makeFormat('p10', 'ns   '), dataMean[1], dataSd[1], frac[1], fracSd[1]))
cat(sprintf(makeFormat('p50', 'ns   '), dataMean[2], dataSd[2], frac[2], fracSd[2]))
cat(sprintf(makeFormat('p90', 'ns   '), dataMean[3], dataSd[3], frac[3], fracSd[3]))
cat(sprintf(makeFormat('μ  ', 'ns   '), dataMean[4], dataSd[4], frac[4], fracSd[4]))
cat(sprintf(makeFormat('τ  ', 'MiB/s'), dataMean[5], dataSd[5], frac[5], fracSd[5]))
