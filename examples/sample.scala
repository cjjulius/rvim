// Scala sample for rvim syntax highlighting.
package demo

object Greeter {
  // A simple case class and a method that folds under foldmethod=syntax.
  case class Point(x: Int, y: Int)

  def distance(a: Point, b: Point): Double = {
    val dx = (a.x - b.x).toDouble
    val dy = (a.y - b.y).toDouble
    math.sqrt(dx * dx + dy * dy)
  }

  def main(args: Array[String]): Unit = {
    val pts = List(Point(0, 0), Point(3, 4))
    val label = """A multi-line
                  |string literal.""".stripMargin
    pts match {
      case first :: second :: _ =>
        println(s"$label -> ${distance(first, second)}")
      case _ =>
        println("not enough points")
    }
  }
}
