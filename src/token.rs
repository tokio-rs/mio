#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Token(pub usize);

use slab;

impl Token {
    #[inline]
    pub fn as_usize(self) -> usize {
        let Token(inner) = self;
        inner
    }
}

impl slab::Index for Token {
    fn from_usize(i: usize) -> Token {
        Token(i)
    }

    fn as_usize(&self) -> usize {
        Token::as_usize(*self)
    }
}
