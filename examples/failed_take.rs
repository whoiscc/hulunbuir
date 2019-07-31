//

use hulunbuir::{Collector, slot::Slot};

mod common;
use common::Node;

fn main() {
    let mut collector = Collector::new(128);
    let slot = collector.allocate(Slot::new(Node(Vec::new())));
    let take_one = collector.take(&slot);
    assert!(take_one.is_ok());
    assert!(collector.take(&slot).is_err());
    collector.fill(&slot, take_one.unwrap());
    assert!(collector.take(&slot).is_ok());
}