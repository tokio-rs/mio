
pub trait Handler<T> {
    fn accept(_token: T) -> Option<T> {
        None
    }

    fn readable(_token: T) {
    }

    fn writable(_token: T) {
    }
}
