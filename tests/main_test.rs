use corust::{make_type_enum, match_t};

make_type_enum! {
    pub enum Shape {
        Circle(f64): Shape,
        Rectangle(f64, f64): Shape,
        Triangle { base: f64, height: f64 }: Shape,
    }
}

#[test]
fn test_enum_to_trait() {
    let figure: &dyn Shape = &Circle(5.0);
    let is_circle = match_t!(figure {
            Circle(_) => true,
            Rectangle(_, _) => false,
            Triangle { .. } => false,
    });

    assert!(is_circle);

    let figure: Box<dyn Shape> = Box::new(Rectangle(4.0, 6.0));
    let area = match_t!(move figure {
            Circle(radius) => 3.14 * radius * radius,
            Rectangle(width, height) => width * height,
            Triangle { base, height } => 0.5 * base * height,
    });

    assert_eq!(area, 24.0);
}

#[test]
fn test_move_non_copy_types() {
    // Test with a non-Copy type to prove we're moving, not copying
    make_type_enum! {
        pub enum Data {
            Text(String): Data,
            Numbers(Vec<i32>): Data,
        }
    }

    let data: Box<dyn Data> = Box::new(Text(String::from("Hello, World!")));
    let message = match_t!(move data {
        Text(s) => s,  // s is String (moved), not &String
        Numbers(v) => format!("{:?}", v),
    });

    assert_eq!(message, "Hello, World!");
    // data is consumed - can't use it anymore

    let data2: Box<dyn Data> = Box::new(Numbers(vec![1, 2, 3]));
    let nums = match_t!(move data2 {
        Text(_) => vec![],
        Numbers(v) => v,  // v is Vec<i32> (moved), not &Vec<i32>
    });

    assert_eq!(nums, vec![1, 2, 3]);
}

#[test]
fn test_enum_generics() {
    make_type_enum! {
        pub enum Either<A, E> {
            Right(A),
            Left(E),
        }
    }

    // Test with concrete types
    let result: Box<dyn Either<i32, String>> = Box::new(Right::<i32>(42));
    let value = match_t!(move result {
        Right<i32>(x) => format!("Success: {}", x),
        Left<i32>(e) => format!("Error: {}", e),
    });
    assert_eq!(value, "Success: 42");

    // Test with error case
    let error: Box<dyn Either<i32, String>> = Box::new(Left::<String>(String::from("oops")));
    let msg = match_t!(move error {
        Right<i32>(x) => format!("Value: {}", x),
        Left<String>(e) => format!("Error: {}", e),
    });
    assert_eq!(msg, "Error: oops");
}

#[test]
fn test_visibility_modifiers() {
    make_type_enum! {
        pub enum Message {
            Text { content: String, sender: String }: Message,
            Info(String): Message,
            Private { content: String }: Message,
        }
    }

    let msg: Box<dyn Message> = Box::new(Text {
        content: String::from("Hello"),
        sender: String::from("Alice"),
    });

    let result = match_t!(move msg {
        Text { content, sender } => format!("{}: {}", sender, content),
        Info(s) => s,
        Private { content } => content,
    });

    assert_eq!(result, "Alice: Hello");
}

make_type_enum! {
    pub enum Term<T: Clone> {
        Lift(T): Term<T>,
        Boolean(bool): Term<bool>,
        Number(i32): Term<i32>,
        Add(Box<dyn Term<i32>>, Box<dyn Term<i32>>): Term<i32>,
        Or(Box<dyn Term<bool>>, Box<dyn Term<bool>>): Term<bool>,
    }

    fn eval(&self) -> T {
        Lift<T>(value) => value.clone(),
        Number(n) => *n,
        Add(left, right) => left.eval() + right.eval(),
        Boolean(b) => *b,
        Or(left, right) => left.eval() || right.eval(),
    }

}

#[test]
fn test_tagless_final() {
    let expr: Box<dyn Term<i32>> = Box::new(Add(
        Box::new(Number(10)),
        Box::new(Add(Box::new(Number(20)), Box::new(Number(5)))),
    ));

    println!("Result: {}", expr.eval());
}

make_type_enum! {
    pub enum Pair<A, B> {
        MkPair(A, B): Pair<A, B>,
        InvertedPair(B, A): Pair<B, A>,
        First(A),
        Second(B),
    }

    fn to_pair(&self) -> (&A, &B) {
        MkPair<A, B>(a, b) => (a, b),
        InvertedPair<B, A>(b, a) => (b, a),
        First<A>(a) => (a, panic!("No second element in First variant")),
        Second<B>(b) => (panic!("No first element in Second variant"), b),
    }

    fn to_pair_owned(self: Box<Self>) -> (A, B) {
        MkPair<A, B>(a, b) => (a, b),
        InvertedPair<B, A>(b, a) => (b, a),
        First<A>(a) => (a, panic!("No second element in First variant")),
        Second<B>(b) => (panic!("No first element in Second variant"), b),
    }

}

#[test]
fn test_multiple_generic_params() {
    let p1: Box<dyn Pair<i32, String>> =
        Box::new(MkPair::<i32, String>(42, String::from("Answer")));
    let p2: Box<dyn Pair<String, i32>> =
        Box::new(InvertedPair::<i32, String>(String::from("Age"), 30));

    match_t!(
        move p1 {
            MkPair<i32, String>(a, b) => {
                assert_eq!(a, 42);
                assert_eq!(b.as_str(), "Answer");
            },
            InvertedPair<String, i32>(_, _) => panic!("Unexpected variant"),
        }
    )
}

#[test]
fn test_boxed_self_method() {
    make_type_enum! {
        pub enum Container {
            Value(String),
            Wrapper(Box<String>),
        }

        fn unwrap(self: Box<Self>) -> String {
            Value(s) => s,
            Wrapper(boxed) => *boxed,
        }
    }

    let c1: Box<dyn Container> = Box::new(Value(String::from("Hello")));
    let result1 = c1.unwrap();
    assert_eq!(result1, "Hello");

    let c2: Box<dyn Container> = Box::new(Wrapper(Box::new(String::from("World"))));
    let result2 = c2.unwrap();
    assert_eq!(result2, "World");
}
