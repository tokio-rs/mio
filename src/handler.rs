
pub trait Handler<T> {
    fn accept(token: T) -> Option<T> {
        None
    }

    fn readable(token: T) {
    }

    fn writable(token: T) {
    }
}
