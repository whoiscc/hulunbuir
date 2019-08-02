
/// Errors thrown by collector.
#[derive(Debug, Fail)]
pub enum Error {
    /// Alive objects count reaches `slot_max` passed to `Collector::new`, and no object
    /// is collectable.
    #[fail(display = "out of slots")]
    OutOfSlots,
    /// Trying to access object with invalid address.
    #[fail(display = "invalid address")]
    InvalidAddress,
    /// Calling `Collector::fill` on non-empty slot. See document of `slot` module for details.
    #[fail(display = "duplicated filling")]
    DuplicatedFilling,
}