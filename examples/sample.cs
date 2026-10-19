// C# syntax-highlighting sample for rvim.
using System;
using System.Collections.Generic;

namespace Demo
{
    /* Keywords, types, strings, numbers and comments each get their own color. */
    public record Point(double X, double Y)
    {
        public double DistanceTo(Point other)
        {
            var dx = X - other.X;
            var dy = Y - other.Y;
            return Math.Sqrt(dx * dx + dy * dy);
        }
    }

    public static class Program
    {
        private const string Greeting = "Hello, rvim";

        public static string Classify(int n) => n switch
        {
            < 0 => "negative",
            0 => "zero",
            _ => "positive",
        };

        public static void Main()
        {
            var points = new List<Point> { new(0, 0), new(3, 4) };
            double total = points[0].DistanceTo(points[1]);
            Console.WriteLine($"{Greeting}: distance = {total}");
            foreach (var n in new[] { -1, 0, 1 })
            {
                Console.WriteLine($"Classify({n}) = {Classify(n)}");
            }
        }
    }
}
