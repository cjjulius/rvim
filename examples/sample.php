<?php
// PHP syntax-highlighting sample for rvim.
declare(strict_types=1);

/* Keywords, types, strings, numbers, variables and comments each get a color. */
final class Point
{
    public function __construct(
        private float $x,
        private float $y,
    ) {}

    public function distanceTo(Point $other): float
    {
        $dx = $this->x - $other->x;
        $dy = $this->y - $other->y;
        return sqrt($dx * $dx + $dy * $dy);
    }
}

function classify(int $n): string
{
    return match (true) {
        $n < 0  => "negative",
        $n === 0 => "zero",
        default  => "positive",
    };
}

$a = new Point(0, 0);
$b = new Point(3, 4);
echo "distance = " . $a->distanceTo($b) . "\n";
foreach ([-1, 0, 1] as $n) {
    printf("classify(%d) = %s\n", $n, classify($n));
}
