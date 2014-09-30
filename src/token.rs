#[deriving(Show, PartialEq, Eq)]
pub struct Token(pub uint);

impl Token {
    #[inline]
    pub fn as_uint(self) -> uint {
        let Token(inner) = self;
        inner
    }
}

// Work around for https://github.com/rust-lang/rust/issues/17169
pub static TOKEN_0:Token = Token(0);
pub static TOKEN_1:Token = Token(1);
