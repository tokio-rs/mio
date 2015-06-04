use util::Index;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Token(pub usize);

impl Token {
    #[inline]
    pub fn as_usize(self) -> usize {
        let Token(inner) = self;
        inner
    }
}

impl Index for Token {
    fn from_usize(i : usize) -> Token {
        Token(i)
    }

    fn as_usize(&self) -> usize {
        Token::as_usize(*self)
    }
}
