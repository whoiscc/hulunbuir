//

use std::cell::RefCell;

use hulunbuir::{Allocator, Collector, Keep, Value};

struct Node(RefCell<Vec<Value<Node>>>);

unsafe impl Keep for Node {
    fn keep(&self) -> Vec<Value<Self>> {
        self.0.borrow().to_owned()
    }
}

fn main() {
    let coll = Collector::new(Node(RefCell::new(Vec::new())), 128);
    let mut allo = Allocator::new(&coll, 16);
    let val1 = allo.allocate(Node(RefCell::new(Vec::new())));
    val1.0.borrow_mut().push(val1);
    assert_eq!(val1.0.borrow().len(), 1);
    assert_eq!(allo.slot_len(), 1);
    allo.clean();
    assert_eq!(allo.slot_len(), 0);
    assert_eq!(coll.slot_len(), 2);
    coll.collect();
    assert_eq!(coll.slot_len(), 1);
    coll.collect();
    assert_eq!(coll.slot_len(), 1);

    let val2 = allo.allocate(Node(RefCell::new(Vec::new())));
    allo.entry().0.borrow_mut().push(val2);
    allo.clean();
    coll.collect();
    assert_eq!(coll.slot_len(), 2);
}
