use std::collections::HashMap;

pub struct Point<T> {
    pub x: T,
    y: Option<T>,
}

impl<T: Copy> Point<T> {
    pub const ORIGIN_X: i32 = 0;

    pub fn new(x: T) -> Self {
        Self { x, y: None }
    }

    fn get(&self) -> T {
        if self.y.is_some() { self.x } else { return self.x; }
    }
}

async fn run(map: &mut HashMap<String, u8>) -> bool {
    let mut n = 0;
    for (k, v) in map.iter_mut() {
        *v += 1; n += k.len();
    }
    match n { 0 => false, _ => true }
}
