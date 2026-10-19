# Ruby syntax-highlighting sample for rvim.
require "json"

# Keywords, strings, numbers, instance variables and method calls get colors.
class Greeter
  attr_reader :name

  def initialize(name)
    @name = name
    @count = 0
  end

  def greet(target = "world")
    @count += 1
    puts "Hello, #{target}, from #{@name} (call #{@count})"
  end
end

def classify(n)
  if n < 0
    "negative"
  elsif n.zero?
    "zero"
  else
    "positive"
  end
end

greeter = Greeter.new("rvim")
%w[world there everyone].each do |who|
  greeter.greet(who)
end

[-1, 0, 1].each { |n| puts "classify(#{n}) = #{classify(n)}" }
