#[test]
fn test_generic_enum() {
    use corust::g;

    g!(
        pub enum Either<L, R> {
            Left: L -> Either<L, R>,
            Right: R -> Either<L, R>,
        }
    );

    let a: &dyn Either<i32, i32> = &Left::new(12);
    let is_left = g!(match a {
        Left(_) => true,
        Right(_) => false,
    });
    assert_eq!(is_left, true);

    let b: &dyn Either<i32, i32> = &Right::new(42);
    let is_right_false = g!(match b {
        Left(_) => false,
        Right(_) => true,
    });
    assert_eq!(is_right_false, true);
}

#[test]
fn test_generic_enum2() {
    use corust::g;

    g!(
        pub enum T<A> {
            D1 : i32 -> T<String>,
            D2 : T<bool>,
            D3 : (A, A) -> T<A>,
        }
    );

    let x: &dyn T<String> = &D1::new(100);
}
