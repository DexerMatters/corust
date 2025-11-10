use corust::{match_t, type_enum};

#[type_enum]
pub enum Shape {
    Circle(f64),
    Rectangle(f64, f64),
    Triangle { base: f64, height: f64 },
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
    #[type_enum]
    pub enum Data {
        Text(String),
        Numbers(Vec<i32>),
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
