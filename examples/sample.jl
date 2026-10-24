# Julia sample for rvim syntax highlighting.
struct Point
    x::Float64
    y::Float64
end

function distance(a::Point, b::Point)
    dx = a.x - b.x
    dy = a.y - b.y
    return sqrt(dx^2 + dy^2)
end

points = [Point(0, 0), Point(3, 4)]
for p in points
    println("distance: ", distance(p, Point(0, 0)))
end
