use enum_typer::{match_t, type_enum};

#[test]
fn test_data() {
    type_enum! {
        enum Product {
            Book { title: String, author: String },
            Electronics { name: String, brand: String },
        }

        fn describe(&self) -> String {
            Book { title, author } => format!("Book: '{}' by {}", title, author),
            Electronics { name, brand } => format!("Electronics: '{}' from {}", name, brand),
        }
    }

    let item1 = Book {
        title: "The Rust Programming Language".to_string(),
        author: "Steve Klabnik".to_string(),
    };
    let item2 = Electronics {
        name: "Smartphone".to_string(),
        brand: "TechBrand".to_string(),
    };

    println!("{}", item1.describe());
    println!("{}", item2.describe());
}

#[test]
fn test_inductive() {
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

    let val1 = Box::new(Inl(42));
    let val2 = Box::new(Inr(true));

    let result1 = fold_sum(val1, |a| a, |b| if b { 1 } else { 0 });
    let result2 = fold_sum(val2, |a| a, |b| if b { 1 } else { 0 });

    assert_eq!(result1, 42);
    assert_eq!(result2, 1);
}

#[test]
fn test_arith() {
    type_enum! {
        enum Arith<T> {
            Num(i32) : Arith<i32>,
            Bool(bool) : Arith<bool>,
            Add(ArithRef<i32>, ArithRef<i32>) : Arith<i32>,
            Mul(ArithRef<i32>, ArithRef<i32>) : Arith<i32>,
            And(ArithRef<bool>, ArithRef<bool>) : Arith<bool>,
            Or(ArithRef<bool>, ArithRef<bool>) : Arith<bool>,
            ToBool(ArithRef<i32>) : Arith<bool>,
            ToNum(ArithRef<bool>) : Arith<i32>,
        }

        fn eval(&self) -> T {
            Num(i) => *i,
            Bool(b) => *b,
            Add(lhs, rhs) => lhs.eval() + rhs.eval(),
            Mul(lhs, rhs) => lhs.eval() * rhs.eval(),
            And(lhs, rhs) => lhs.eval() && rhs.eval(),
            Or(lhs, rhs) => lhs.eval() || rhs.eval(),
            ToBool(expr) => expr.eval() != 0,
            ToNum(expr) => if expr.eval() { 1 } else { 0 },
        }
    }

    type ArithRef<T> = Box<dyn Arith<T>>;

    fn eval2(expr: ArithRef<i32>) -> i32 {
        match_t!(move expr {
            Add(lhs, rhs) => eval2(lhs) + eval2(rhs),
            Mul(lhs, rhs) => eval2(lhs) * eval2(rhs),
            ToNum(e) => if eval1(e) { 1 } else { 0 },
            Num(i) => i,
        })
    }

    fn eval1(expr: ArithRef<bool>) -> bool {
        match_t!(move expr {
            And(lhs, rhs) => eval1(lhs) && eval1(rhs),
            Or(lhs, rhs) => eval1(lhs) || eval1(rhs),
            ToBool(e) => eval2(e) != 0,
            Bool(b) => b,
        })
    }

    let expr: ArithRef<_> = Box::new(Add(
        Box::new(Num(10)),
        Box::new(Mul(Box::new(Num(2)), Box::new(Num(3)))),
    ));

    let expr2: ArithRef<_> = Box::new(And(
        Box::new(ToBool(Box::new(Num(0)))),
        Box::new(ToBool(Box::new(Num(5)))),
    ));

    assert_eq!(expr.eval(), 16);

    assert_eq!(expr2.eval(), false);

    assert_eq!(eval2(expr), 16);

    assert_eq!(eval1(expr2), false);
}

#[test]
fn test_safe_list() {
    struct Empty;
    struct NonEmpty;

    type_enum! {
        enum SafeList<T, E> {
            Nil : SafeList<T, Empty>,
            Cons(T, SafeListRef<T, E>) : SafeList<T, NonEmpty>,
        }
    }

    fn safe_head<T: 'static>(list: SafeListRef<T, NonEmpty>) -> T {
        match_t!(move list {
            Cons<T, NonEmpty>(head, _tail) => head,
        })
    }

    type SafeListRef<T, E> = Box<dyn SafeList<T, E>>;

    let list: SafeListRef<_, _> = Box::new(Cons(1, Box::new(Cons(2, Box::new(Nil)))));

    let head = safe_head(list);
    assert_eq!(head, 1);
}

#[test]
fn test_field_generics() {
    type_enum! {
        enum Nat {
            Zero,
            Succ<N: Nat>(N) : Nat,
        }

        fn to_u32(&self) -> u32 {
            Zero => 0,
            Succ<N: Nat>(n) => 1 + n.to_u32(),
        }
    }

    let three = Succ(Succ(Succ(Zero)));
    assert_eq!(three.to_u32(), 3);
}

#[test]
fn test_field_generics_arith() {
    type_enum! {
        enum Arith<T> {
            Num(i32) : Arith<i32>,
            Bool(bool) : Arith<bool>,
            Add<A: Arith<i32>, B: Arith<i32>>(A, B) : Arith<i32>,
            Or<A: Arith<bool>, B: Arith<bool>>(A, B) : Arith<bool>,
        }

        fn eval(&self) -> T {
            Num(i) => *i,
            Bool(b) => *b,
            Add<A, B>(a, b) => a.eval() + b.eval(),
            Or<A, B>(a, b) => a.eval() || b.eval(),
        }
    }

    let expr = Add(Num(10), Add(Num(20), Num(5)));

    let expr2 = Or(Bool(false), Or(Bool(true), Bool(false)));

    assert_eq!(expr.eval(), 35);

    assert_eq!(expr2.eval(), true);
}
