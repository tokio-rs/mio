#[deriving(Copy, Show, PartialEq, Eq, Hash)]
pub struct Token(pub uint);

impl Token {
    #[inline]
    pub fn as_uint(self) -> uint {
        let Token(inner) = self;
        inner
    }
}
