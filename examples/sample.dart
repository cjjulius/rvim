// Dart sample for rvim syntax highlighting.
import 'dart:math';

class Point {
  final double x;
  final double y;
  const Point(this.x, this.y);

  double distanceTo(Point other) {
    final dx = x - other.x;
    final dy = y - other.y;
    return sqrt(dx * dx + dy * dy);
  }
}

Future<void> main() async {
  final points = <Point>[const Point(0, 0), const Point(3, 4)];
  final label = '''A multi-line
string literal.''';

  for (final p in points) {
    print('$label origin distance: ${p.distanceTo(const Point(0, 0))}');
  }
}
