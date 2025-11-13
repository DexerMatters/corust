# enum-typer

Type-indexed enums, pattern matching and GADTs for Rust


A procedural macro library that brings Generalized Algebraic Data Types (GADTs) to Rust through a powerful `type_enum!` macro. Define enums where each variant can refine the overall type, enabling compile-time type safety for complex data structures like type-safe expression trees, length-indexed vectors, and state machines.

[![Crates.io](https://img.shields.io/crates/v/enum-typer.svg)](https://crates.io/crates/enum-typer)
[![Documentation](https://docs.rs/enum-typer/badge.svg)](https://docs.rs/enum-typer)
[![License: MIT OR Apache-2.0](https://img.shields.io/crates/l/enum-typer.svg)](README.md#license)

## Features

- 🎯 **Type Indexing** - Each variant can specify different type parameters, like GADTs in Haskell/OCaml
- 🔒 **Phantom Types** - Track compile-time state (empty/non-empty lists, type-level naturals)
- 📦 **Trait Objects** - Automatic trait generation with `Box<dyn Trait>` support for existential types
- 🔄 **Pattern Matching** - Runtime type-based matching with `match_t!` macro
- 🎨 **Methods** - Define methods directly in the enum with type-indexed return types
- 🧬 **Variant Generics** - Each variant can have its own generic parameters with trait bounds
- ⚡ **Smart Inference** - Automatic type parameter filtering and PhantomData injection

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
enum-typer = "0.1.0"
```

### Basic Example

```rust
use enum_typer::type_enum;

type_enum! {
    enum Either<A, B> {
        Left(A),
        Right(B),
    }
}

// Use as trait objects
let value: Box<dyn Either<i32, String>> = Box::new(Left(42));
```

## Type Indexing (GADTs)

Define enums where variants constrain the overall type parameter:

```rust
type_enum! {
    enum Either<A, B, Tag> {
        Left(A) : Either<A, B, LeftTag>,
        Right(B) : Either<A, B, RightTag>,
    }
}

struct LeftTag;
struct RightTag;

type EitherRef<A, B, Tag> = Box<dyn Either<A, B, Tag>>;

// Type system proves this value is Left
let value: EitherRef<i32, String, LeftTag> = Box::new(Left(42));

// Can't compile - type mismatch!
// let wrong: EitherRef<i32, String, LeftTag> = Box::new(Right("hello".to_string()));
```

Each variant acts as a type-level proof of which case you have. The `Tag` parameter is refined by the variant constructor.

## Phantom Types for Compile-Time Safety

Track state at the type level to prevent runtime errors:

```rust
struct Empty;
struct NonEmpty;

type_enum! {
    enum SafeList<T, E> {
        Nil : SafeList<T, Empty>,
        Cons(T, SafeListRef<T, E>) : SafeList<T, NonEmpty>,
    }
}

type SafeListRef<T, E> = Box<dyn SafeList<T, E>>;

// This function only accepts non-empty lists
fn safe_head<T: 'static>(list: SafeListRef<T, NonEmpty>) -> T {
    match_t!(move list {
        Cons<T, NonEmpty>(head, _tail) => head,
    })
}

let list: SafeListRef<i32, _> = Box::new(
    Cons(1, Box::new(Cons(2, Box::new(Nil))))
);

let head = safe_head(list); // ✓ Compiles
// safe_head(Box::new(Nil)); // ✗ Compile error!
```

## Pattern Matching with `match_t!`

Runtime pattern matching on trait objects:

```rust
type_enum! {
    enum Sum<A, B> {
        Inl(A),
        Inr(B),
    }
}

type SumRef<A, B> = Box<dyn Sum<A, B>>;

fn fold_sum<A, B, R>(sum: SumRef<A, B>, f_inl: fn(A) -> R, f_inr: fn(B) -> R) -> R
where
    A: 'static,
    B: 'static,
{
    match_t!(move sum {
        Inl<A>(a) => f_inl(a),
        Inr<B>(b) => f_inr(b),
    })
}

let val = Box::new(Inl(42));
let result = fold_sum(val, |a| a * 2, |b| if b { 1 } else { 0 });
assert_eq!(result, 84);
```

## Methods and Existential Returns

Define methods that return type-indexed results. The return type `T` is existentially quantified - it depends on which variant you have:

```rust
type_enum! {
    enum Arith<T> {
        Num(i32) : Arith<i32>,
        Bool(bool) : Arith<bool>,
        Add(ArithRef<i32>, ArithRef<i32>) : Arith<i32>,
        Or(ArithRef<bool>, ArithRef<bool>) : Arith<bool>,
    }
    
    fn eval(&self) -> T {
        Num(i) => *i,
        Bool(b) => *b,
        Add(lhs, rhs) => lhs.eval() + rhs.eval(),
        Or(lhs, rhs) => lhs.eval() || rhs.eval(),
    }
}

type ArithRef<T> = Box<dyn Arith<T>>;

let expr: ArithRef<i32> = Box::new(
    Add(Box::new(Num(10)), Box::new(Num(5)))
);
assert_eq!(expr.eval(), 15);  // Returns i32

let bool_expr: ArithRef<bool> = Box::new(
    Or(Box::new(Bool(true)), Box::new(Bool(false)))
);
assert_eq!(bool_expr.eval(), true);  // Returns bool
```

The type system ensures you can't mix incompatible types - `Add` only accepts `Arith<i32>`, not `Arith<bool>`. The return type of `eval()` changes based on the type index `T`.

## How It Works

The `type_enum!` macro transforms your enum definition into:

1. **A trait** with the enum's name containing any defined methods
2. **Separate structs** for each variant
3. **Trait implementations** with smart generic filtering to avoid unconstrained type parameters
4. **Automatic PhantomData** injection for phantom type parameters

### Example Transformation

```rust
type_enum! {
    enum SafeList<T, E> {
        Nil : SafeList<T, Empty>,
        Cons(T, Box<dyn SafeList<T, E>>) : SafeList<T, NonEmpty>,
    }
}
```

Expands approximately to:

```rust
trait SafeList<T, E>: std::any::Any {}

struct Nil;
impl<T: 'static> SafeList<T, Empty> for Nil {}

struct Cons<T, E>(T, Box<dyn SafeList<T, E>>);
impl<T: 'static, E: 'static> SafeList<T, NonEmpty> for Cons<T, E> {}
```

Notice how `Nil` only has `impl<T>`, not `impl<T, E>` - the macro automatically filters unused type parameters.

## Advanced Features

### Variant-Level Generics

Variants can have their own generic parameters, independent of the enum's generics:

```rust
type_enum! {
    enum Container {
        Simple<T>(T) : Container,
        Nested<U: Container>(Box<U>) : Container,
    }
}

// Each variant is a struct with its own generics:
// struct Simple<T: 'static>(T);
// struct Nested<U: Container + 'static>(Box<U>);

let value: Box<dyn Container> = Box::new(Simple(42));
let nested: Box<dyn Container> = Box::new(Nested(value));
```

This enables:
- **Self-referential types**: Variants can reference the enum trait itself with different type parameters
- **Per-variant constraints**: Each variant can have its own trait bounds (e.g., `U: Container`)
- **Flexible composition**: Build complex recursive structures without cluttering the main enum signature

```rust
type_enum! {
    enum Expression {
        Value<T>(T) : Expression,
        Lambda<F: Fn(i32) -> i32>(F) : Expression,
    }
}

let val = Box::new(Value(42));
let func = Box::new(Lambda(|x| x * 2));
```

### Trait Bounds Preservation

Type parameter bounds are automatically preserved:

```rust
trait Nat {
    type Pred: Nat;
}

type_enum! {
    enum SafeVector<T, N: Nat> {
        VNil : SafeVector<T, Zero>,
        VCons(T, SafeVectorRef<T, N::Pred>) : SafeVector<T, Succ<N>>,
    }
}

// Generated impl preserves N: Nat bound
// impl<T: 'static, N: Nat + 'static> SafeVector<T, Succ<N>> for VCons<T, N> { ... }
```

## Limitations

- **Inference limits**: Associated types like `N::Pred` may require explicit type annotations
- **'static bound**: All type parameters require `'static` for trait object compatibility
- **No exhaustiveness**: `match_t!` panics on unmatched patterns (no compile-time exhaustiveness checking)

## Examples

See the `tests/examples.rs` file for more examples including:
- Type-safe arithmetic expression trees
- Empty/non-empty lists with compile-time guarantees
- Length-indexed vectors with type-level naturals
- Sum types with generic folding
- Variants with their own generic parameters and trait bounds

## License

MIT OR Apache-2.0
