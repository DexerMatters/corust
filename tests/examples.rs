use type_enum::{match_t, type_enum};

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

        fn eval(self: &Self) -> T {
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
