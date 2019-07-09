//

use std::sync::RwLock;

extern crate hulunbuir;
use hulunbuir::{Allocator, Collector, Keep, Value};

struct Node(Vec<Value<Node>>);

impl Keep for Node {
    fn keep(&self) -> &[Value<Self>] {
        &self.0
    }
}

fn main() {
    let coll = Collector::new(Node(Vec::new()), 128);
    let coll_lock = RwLock::new(coll);
    let mut allo = Allocator::new(&coll_lock, 16);
    let val1 = allo.allocate(Node(Vec::new()));
    unsafe {
        val1.get_mut().0.push(val1);
        assert_eq!(val1.get_mut().0.len(), 1);
    }
}
