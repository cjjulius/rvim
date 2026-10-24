#!/usr/bin/perl
# Perl sample for rvim syntax highlighting.
use strict;
use warnings;

sub distance {
    my ($x1, $y1, $x2, $y2) = @_;
    my $dx = $x1 - $x2;
    my $dy = $y1 - $y2;
    return sqrt($dx * $dx + $dy * $dy);
}

my @points = ([0, 0], [3, 4]);
foreach my $p (@points) {
    printf("distance: %.2f\n", distance(@$p, 0, 0));
}
