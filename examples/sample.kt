// Kotlin syntax-highlighting sample for rvim.
package demo

import kotlin.math.sqrt

/*
 * Block comments, line comments, strings and numbers all get their own color.
 */
data class Point(val x: Double, val y: Double) {
    fun distanceTo(other: Point): Double {
        val dx = x - other.x
        val dy = y - other.y
        return sqrt(dx * dx + dy * dy)
    }
}

const val GREETING = "Hello, rvim"

val banner = """
    Triple-quoted raw strings
    can span multiple lines.
"""

fun classify(n: Int): String = when {
    n < 0 -> "negative"
    n == 0 -> "zero"
    else -> "positive"
}

fun main() {
    val points = listOf(Point(0.0, 0.0), Point(3.0, 4.0))
    val total = points.first().distanceTo(points.last())
    println("$GREETING: distance = $total")
    for (n in -1..1) {
        println("classify($n) = ${classify(n)}")
    }
}
