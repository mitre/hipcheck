---
title: "Policy Expression Typing"
---

## The Current Situation

Expressions in the policy expression language used by Hipcheck do not get
type-checked until they are evaluated, similar to an interpreted language.
Hipcheck analysis takes multiple orders of magnitude longer to complete than it
takes to evaluate a policy expression. Because of this, it can be frustrating
for people writing policy expressions to wait until after an analysis has run to
find out they had a type error in their expression. It would be nice to be able
to vet the expressions in a policy file as much as possible before running
analysis to avoid these types of situations.

The current design of policy expressions does not have a robust type system, as
evidenced by the inability to do pre-evaluation type checking, and the inability
to run-time cast between primitive types, which lead to [Issue #449](https://github.com/mitre/hipcheck/issues/449).

In this bug, if Hipcheck analysis query happened to return a float that
ended in `.0` and was therefore a whole number, when serialized then
deserialized and inserted into a policy expression, it would be treated as an
integer, since JSON does not distinguish between floats and ints. The binary
arithmetic functions (e.g. `add`) in the policy expression standard environment
expect two like-typed primitives (float + float, int + int), and so would throw
a runtime type error when a would-be float operand was late-bound as an integer.

```
(lte $ 0.2) , "0.0" --> (lte 0 0.2) --> Error::BadType
```

This particular issue was solved by introducing an `upcast` function that could
turn a `Primitive::Int` into a `Primitive::Float`, and then changing the
behavior of `binary_primitive_op()` to detect a "one int, one float" situation,
and dynamically promote the integer operand.

However, it would be nicer and cleaner to be able to type-check and upcast
primitives across the entire expression before evaluation. For example, allowing
`[0 1 2.0 3]` by upcasting all non-float elements to floats.

## The Goal

Briefly, the goal of this RFD is to describe a refactor of the Policy Expression
system that enables "compile"-time type checking as much as possible, and makes
it in general to write Expr-manipulating functionality using the `Visitor`
pattern.

### What Would Need To Happen

1. Introduce first-class types and a uniform way to get the type of a given
   `Expr/Primitive`
2. Represent the intention to cast a primitive in the Expr struct ecosystem.
3. Type information associated with functions/variables added to an `Env`
   instance.
4. `Env` instance available when type-checking to grab information about a
   function/variable.
5. Implement `TypeChecker` as a struct that implements a new `ExprVisitor` trait
6. Re-implement expression evaluation as a `impl ExprVisitor`

## Proposal

### Extracting `Expr` Variants into Types

First, we move out variants of `Expr` with fields into their own structs. E.g.

```rust
Expr::Array(Vec<Expr>)

// Becomes

struct Array {
	elts: Vec<Expr>
}
Expr::Array(Array)
```

The benefit of this is that we can implement traits on each `Expr` variant, and
add different `Visitor` functions for each. Without the above example change to
`Array`, you can't write a `fn visit_array(array: ?)` that is different from `fn
visit_expr(expr: Expr)` because there is no internal type to unwrap and
therefore distinguish it.

We do this same change to `Function`, and `Lambda`.

### Adding Typing

We then add a `enum Type` to capture the types of all expressions/primitives as
follows.

```rust
type PrimitiveType = std::mem::Discriminant<Primitive>;
type ArrayType = Option<std::mem::Discriminant<Primitive>>;
struct FunctionType {
    pub args: Vec<Box<Type>>,
    pub output: Box<Type>,
}
enum Type {
    Primitive(PrimitiveType),
    Array(ArrayType),
    Function(FunctionType),
    ...
}
```

For primitives, we use the `Discriminant` of the primitive enum. As arrays can
only contain primitives, they also use the `Primitive` discriminant, but since
we can't know at compile time the type of an empty array or one whose only
element is a `JSONPointer`, it is an `Option`.

A `FunctionType` is a combination of the array of types of its inputs and
outputs. This will be somewhat difficult to resolve with the existing `Env`
system, as many functions do implicit overloading (e.g. the same `add` function
can handle int, float, and span types), so the output type is dependent on the
input type. We should consider dynamically retrieving the type of a function by
passing a `&Env` reference.

With this `Type` information represented, we can now add and implement the
`Typed` trait:

```rust
trait Typed {
    fn typ(&self) -> Type;
}
impl Typed for Primitive {
    fn typ(&self) -> Type {
        Type::Primitive(std::mem::discriminant(self))
    }
}
impl Typed for Expr {
	...
}
```

Notably because of an overloaded function's need to check its own arguments
against the function implementation, we'll either need to augment
`Expr::Function` to contain some reference to the actual underlying function in
`Env` or to be able to query `Env` about that function when `Typed::typ()` is
being executed.

### Adding `Cast` Type

Now to be able to represent a cast operation in the `Expr` ecosystem. Following
the above rules about creating distinct struct, we add the following to the
`Expr` enum:

```rust
struct Cast {
	target: PrimitiveType,
	expr: Box<Expr>,
}

Expr::Cast(Cast)

impl Typed for Cast {
	fn typ(&self) -> Type {
		Type::Primitive(self.target)
	}
}

```

With this tool, when we perform type checking / upcasting as a stage of
expression compilation, `Function` and `Array` instances can insert `Cast`
nodes to "wrap" improperly typed primitives. This would allow `(lte 0 0.2)` to
be evaluated properly, as the function replaces its first operand with
`Cast::new(op2.typ(), op1)`. There will need to be some `try_upcast() ->
Result<Expr>` function to determine whether a cast can be done according to the
semantics of the language.

### The `ExprVisitor` Trait

To re-organize computation/tranformation of the `Expr` tree generated by a
policy expression program, we can use the `Visitor` pattern. We define a trait
as such:

```rust
trait ExprVisitor<T> {
	fn visit(&self, expr: &mut Expr) -> T;
	fn visit_array(&self, arr: &mut Array) -> T;
	fn visit_function(&self, f: &mut Function) -> T;
	fn visit_primitive(&self, p: &mut Primitive) -> T;
	fn visit_cast(&self, c: &mut Cast) -> T;
	...
}
```

Note that the separation of different functions to handle different `Expr`
variants is enabled by splitting them out into distinct types wrapped by their
`Expr` variant.

We can write multiple structs that implement `ExprVisitor`.

1. `struct InsertCast` which inserts `Cast` nodes where applicable
2. `struct TypeCheck` which returns `Result<()>` to report a type error.
3. `struct JsonInjector` which takes `context` and replaces all `Expr::JsonPointer` accordingly.
4. `struct Executor`. Re-impl the existing `Executor` struct to obey this pattern.

This would allow us to organize our transformation steps more effectively. When
we first parse a policy expression from file, we can call `InsertCast,
TypeCheck` to make sure everything pre-context-injection is sound, with some
ambiguity allowed around the JSON pointers themselves. Then once analysis
completes we can `JsonInjector/InsertCast/TypeCheck` again, then finally call
`Executor`'s `visit()` function to return the evaluated `Expr`.

### Type of Overloaded Functions

The main remaining issue is that currently `Expr::Function` maintains only a
function identifer and arguments, but with no connection to the code that will
actually be run in its place. This is because `Env` contains the functions, and
it is not required to exist at parsing time. There are two possible solutions
for adding "behavior awareness" to functions. For one, we could update the
parsing code to require a `Env` and remove that from `Executor`, and update
`Expr::Function` to contain `Op` (the "behavior") directlty. At the moment, we
don't really allow Hipcheck users to supply their own `Env` or add functions to
`Env`, so we know what `Env` will be from the start of execution, and its no
trouble to have an instance of `Env` ready for policy file/expression parsing.

However, if we wanted to allow for a delayed function resolution closer to the
way we have it now, we could add an additional `Expr::ResolvedFunction` type
which is like the existing `Expr::Function` with an additional `Op` field.
`Expr::ResolvedFunction` would be able to return a type for `trait Typed`, but
`Expr::Function` would not, and we'd either need to `panic!()` or change
`Typed::typ()` to return `Result<Type>`.

In either case, we still need to augment `Op` to be able to return information
its return type. `Op` should take an additional field that is an instance of an enum such as

```rust
enum OpReturnType {
 	Static(Expr),
	Overloaded(fn(args: Vec<Type>) -> Type)
}
```

So that functions which always return the same thing (e.g. `(count)` always returns an int), can specify that succinctly, while overloaded functions can use an additional function to describe what the return type would be.
