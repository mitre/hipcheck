---
title: Policy Expressions
weight: 2
---

# Policy Expressions

"Policy expressions" are a small expression language in Hipcheck that allows the
JSON data output by analysis plugins to be reduced to a boolean pass/fail
determination used for scoring. Policy expressions are mostly found in policy
files, as the `policy` node for analyses or the `investigate` node for the
entire analysis. Plugin authors may also want to be familiar with policy
expressions, as one of the gRPC calls they may implement returns a default
policy expression for the analysis implemented by the plugin.

The policy expression language is limited. It does not permit user-defined
functions, assignment to variables, or the retention of any state. Any policy
expression supplied in a policy file which does not result in a boolean output
will produce an error.

If the policy expression language is insufficient to represent a desired policy
on the output of a given plugin, users are encouraged to write their own plugin
which takes as input that plugin's output and performs the desired manipulation.

### Primitives

Policy expressions have the following primitive types:

| Type    | Description | Example |
| ------- | ----------- | ------- |
| integer | A signed 64-bit integer | `-5`, `360` |
| float | A 64-bit float, NaN is disallowed | `2.001` |
| boolean | A true or false value | `#t`, `#f` |
| identifier | A function name or placeholder value in a lambda function | `add` |
| datetime | A datetime value with timezone information. [More info](#datetime) | `2024-09-17T09:00-05` |
| span | a (uniform) duration of time. [More info](#span) | `P5wT1h30m` |

#### Datetime

Datetimes use the string format from the `jiff` [crate][jiff], which is a
lightly modified version of ISO8601. A datetime must include a date in the
format `<YYYY>-<MM>-<DD>`. An optional time in the format `T<HH>:[MM]:[SS]` will
be accepted after the date. Decimal fractions of hours and minutes are not
allowed; use smaller time units instead (e.g. `T10:30` instead of `T10.5`).
Decimal fractions of seconds are allowed. The timezone is always internally
resolved to UTC, but you can specify the datetime's original timezone as an an
offset from UTC by including `+{HH}:[MM]` or `-{HH}:[MM]`.

#### Span

Spans represent a duration of time using the `jiff` [crate] `Span` type. Policy
expression spans can include weeks, days, hours, minutes, and seconds. They can
include optional decimal fractions of the smallest unit of time (hours, minutes,
or seconds) used (e.g. `1.5h`). Spans are prefixed with the letter "P" followed
by optional date units. Time units are separated from date units with the letter
"T". All date and time units are specified in single case-agnostic letter
abbreviations after the number. For example, a span of one week, one day, one
hour, one minute, and one-and-a-tenth seconds would be `P1w1dT1h1m1.1s`.

Although `jiff` day and week spans can be non-uniform depending on timezone
information, policy expression spans always use uniform 24-hour days and 7-day
weeks.

### Expressions

#### Arrays

Arrays are vectors of homogeneously-type primitives. This means that all
elements of an array must be the same type, and that type must be a primitive
(integer, float, boolean, datetime, span). Arrays cannot contain expression
types like other arrays, functions, or lambdas. Square brackets represent the
array boundaries and elements are separated by whitespace. Examples:

 ```
 [1 1 2 3 5 8]
 [0.152, -12.482, 0.09]
 [#t #t #f #t #f]
 ```

#### Function

Functions are Lisp-like expressions, meaning that they are bounded by
parentheses, and the function name comes first followed by whitespace-delimited
operands. Examples:

```
(add 2 2) // Add two integers
(min [-3.1, -6.6, 7.8]) // Get the minimum of an array of floats
```

##### Primitive Function Reference

The standard environment for evaluating policy expressions contains the
following functions:

| Function | Name | Operand Types | Behavior |
| ---------| ---- | -------- | -------- |
| `(gt <A> <B>)`| greater than | non-identifier primitives | evaluate `A > B` |
| `(lt <A> <B>)`| less than | non-identifier primitives | evaluate `A < B` |
| `(gte <A> <B>)`| greater than or equal | non-identifier primitives | evaluate `A >= B` |
| `(lte <A> <B>)`| less than or equal | non-identifier primitives | evaluate `A <= B` |
| `(eq <A> <B>)` | equal | non-identifier primitives | evaluate `A == B` |
| `(neq <A> <B>)` | not equal | non-identifier primitives | evaluate `A != B` |
| `(add <A> <B>)` | add | integers, floats, bools, spans, or (datetime + span) | evaluate `A + B` |
| `(sub <A> <B>)` | subtract | integers, floats, bools, spans, or (datetime + span) | evaluate `A - B` |
| `(divz <A> <B>)` | divide or zero | integers, floats | if `B == 0` return `B`, else evaluate `A / B` |
| `(duration <A> <B>)` | duration | datetimes | evaluate `A - B` to produce a `span` |
| `(and <A> <B>)` | and | bools | evaluate `A & B` |
| `(or <A> <B>)` | or | bools | evaluate `A | B` |
| `(not <A>)` | not | bool | evaluate `!A` |
| `(max <A>)` | max | array of integers, floats, datetimes, spans | find the largest value in `A` |
| `(min <A>)` | min | array of integers, floats, datetimes, spans | find the smallest value in `A` |
| `(avg <A>)` | average | array of integers, floats | calculate the average of `A` |
| `(median <A>)` | median | array of integers, floats | calculate the median of `A` |
| `(count <A>)` | count | array of non-identifier primitives | return the number of elements in `A` |

#### Lambdas

A lambda is an incomplete function invocation that is missing an operand. In the
standard policy expression environment, there are multiple functions that take
as operands a lambda and an array, and then evaluate the lambda
for each element of the array. For example, `(lte 8.0)` is an incomplete `lte`
function call. When we do the following:

```
(foreach (lte 8.0) [0.3, 9.4, 5.1])
```

It will apply the lambda to each element of the float array, resulting in an
array of three booleans that correspond to whether the element at that index in
the float array was less than `8.0`.

Note that for this to work, the array element is inserted as the first operand
in a binary operand function.


##### Lambda Function Reference

Each function takes a lambda as the first operand and an array as the second.
The type of the array and the type of the missing operand in the lambda must
match.

| Function | Name | Behavior |
| `(all <A> <B>)` | all | return `#t` if `A` returned `#t` for all elements of `B` |
| `(nall <A> <B>)` | not all | return `#t` if `A` returned `#f` for at least one element of `B` |
| `(some <A> <B>)` | some | return `#t` if `A` returned `#t` for at least one element of `B` |
| `(none <A> <B>)` | none | return `#t` if `A` returned `#f` for all elements of `B` |
| `(filter <A> <B>)` | filter | return the subset of elements of `B` for which `A` returned `#t` |
| `(foreach <A> <B>)` | for each | apply `A` to each element of `B`, producing a same-size array |

Some examples:

```
(filter (gt 10) [3 11 0]) // Return array of elements less than or equal to 10
(foreach (not) [#t #f]) // Return an array of inverted booleans
(some (gt 10) [3 11 0]) // Return true if any element is less than or equal to 10
```

#### JSON Pointers

As a reminder, the purpose of the policy expression language is to allow us to
manipulate data from plugins and produce a boolean pass/fail determination. Each
policy expression in a Hipcheck policy file needs to contain one or more
locations at which to "receive" part or all of the JSON data from a plugin
(otherwise the policy would be independent of the data and could be evaluated
immediately). This is where JSON pointers come in.

A JSON pointer is a replacement for an expression or function operand in a
policy expression. They are prefixed with a `$`. If the JSON value is an object,
fields can be recursively accessed by appending `/<FIELD_NAME>`. For example,
to extract the float at field "baz" below, we would use `$/bar/baz`:

```
{
	"foo": [1, 2, 3, 4],
	"bar": {
		"bee": false,
		"baz": 0.01
	}
}
```

Examples:

|Plugin Output | Goal | Policy Expression
|----|----|----|
| A boolean value | Forward the value as the pass/fail determination | `$` |
| A JSON array | Pass if all elements less than 10 | `(all (gt 10) $)` |
| An object containing a boolean field "fail" | Invert the field | `(not $/fail)` |

As mentioned above, a policy expression can contain multiple JSON
pointers. As an example, this can be useful if you want to calculate the
percentage of elements of an array that pass a filter:

```
(lt (divz (count (filter (lt 10) $) (count $))) 0.5)
```

This policy expression will check that less than half of the elements in `$` are
less than 10.  It uses JSON pointers twice, once to get the total element count,
again to count the number of elements filtered by the lambda.

[jiff]: https://crates.io/crates/jiff
