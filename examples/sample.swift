// Swift syntax-highlighting sample for rvim.
import Foundation

/* Keywords, types, strings, numbers and comments each get their own color. */
struct Point {
    let x: Double
    let y: Double

    func distance(to other: Point) -> Double {
        let dx = x - other.x
        let dy = y - other.y
        return (dx * dx + dy * dy).squareRoot()
    }
}

let greeting = """
    Triple-quoted strings
    can span multiple lines.
    """

func classify(_ n: Int) -> String {
    switch n {
    case ..<0: return "negative"
    case 0: return "zero"
    default: return "positive"
    }
}

let points = [Point(x: 0, y: 0), Point(x: 3, y: 4)]
let total = points[0].distance(to: points[1])
print("\(greeting): distance = \(total)")
for n in [-1, 0, 1] {
    print("classify(\(n)) = \(classify(n))")
}
