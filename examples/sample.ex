# Elixir sample for rvim syntax highlighting.
defmodule Geometry do
  @moduledoc """
  Small geometry helpers.
  """

  @doc "Euclidean distance between two points."
  def distance({x1, y1}, {x2, y2}) do
    dx = x1 - x2
    dy = y1 - y2
    :math.sqrt(dx * dx + dy * dy)
  end

  def describe(points) do
    points
    |> Enum.map(&distance(&1, {0, 0}))
    |> Enum.each(fn d -> IO.puts("from origin: #{d}") end)
  end
end
