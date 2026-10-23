-- Haskell sample for rvim syntax highlighting.
module Main (main) where

import Data.List (sort)

-- | Euclidean-ish distance between two points.
distance :: (Double, Double) -> (Double, Double) -> Double
distance (x1, y1) (x2, y2) =
  let dx = x1 - x2
      dy = y1 - y2
  in sqrt (dx * dx + dy * dy)

greet :: String -> String
greet name = "Hello, " ++ name ++ "!"

main :: IO ()
main = do
  let points = sort [3, 1, 2] :: [Int]
  mapM_ print points
  putStrLn (greet "world")
