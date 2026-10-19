-- Lua syntax-highlighting sample for rvim.
--[[
  Long comments open with --[[ and run until the closing bracket,
  spanning as many lines as you like.
]]

local Point = {}
Point.__index = Point

function Point.new(x, y)
  return setmetatable({ x = x, y = y }, Point)
end

function Point:distance_to(other)
  local dx = self.x - other.x
  local dy = self.y - other.y
  return math.sqrt(dx * dx + dy * dy)
end

local function classify(n)
  if n < 0 then
    return "negative"
  elseif n == 0 then
    return "zero"
  else
    return "positive"
  end
end

local a = Point.new(0, 0)
local b = Point.new(3, 4)
print("distance = " .. a:distance_to(b))

for _, n in ipairs({ -1, 0, 1 }) do
  print("classify(" .. n .. ") = " .. classify(n))
end
