//

use hulunbuir::{Address, Collector, Keep, slot::Slot};

struct Node(Vec<Address>);

unsafe impl Keep for Node {
    fn with_keep<F: FnOnce(&[Address])>(&self, f: F) {
        f(&self.0)
    }
}

fn main() {
    let mut collector = Collector::new(128);
    let slot = collector.allocate(Slot::new(Node(Vec::new())));
    let take_one = collector.take(&slot);
    assert!(take_one.is_ok());
    assert!(collector.take(&slot).is_err());
    collector.fill(&slot, take_one.unwrap());
    assert!(collector.take(&slot).is_ok());
}