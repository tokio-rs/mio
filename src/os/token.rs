#[derive(Copy, Clone, Show, PartialEq, Eq, Hash)]
pub struct Token(pub usize);

impl Token {
    #[inline]
    pub fn as_usize(self) -> usize {
        let Token(inner) = self;
        inner
    }
}
