 /** 
 * An internal token generator which generates sequentially a mapped token.
 * 
 * A token identification table
 * 
 * Token ID mappings: 
 * 
 * ```text
 +----------+---------------------------------------------+-------+---+---+---+---+---+---+---+
 | Type     | Mask                                        | 7H 7L | 6 | 5 | 4 | 3 | 2 | 1 | 0 |
 +----------+---------------------------------------------+-------+---+---+---+---+---+---+---+
 | AFD      | 0x0000_0000_0000_0000-0x5FFF_FFFF_FFFF_FFFF | 0-2 x | X | X | X | X | X | X | X |
 | Pipe     | 0x6000_0000_0000_0000-0xBFFF_FFFF_FFFF_FFFF | 3-5 x | X | X | X | X | X | X | X |
 | Event    | 0xC000_0000_0000_0000-0xDFFF_FFFF_FFFF_FFFF | 6   x | X | X | X | X | X | X | X |
 | Waker    | 0xE000_0000_0000_0000-0xFFFF_FFFF_FFFF_FFFF | 7   x | X | X | X | X | X | X | X |
 +----------+---------------------------------------------+-------+---+---+---+---+---+---+---+
 * ```
 * 0x0000_0000_0000_0000-0x5FFF_FFFF_FFFF_FFFF = 6 917 529 027 641 081 855
 */


use std::{fmt, marker::PhantomData, ops::RangeInclusive};


/// A realization of the token generator using atomics.
#[cfg(target_has_atomic = "ptr")]
pub mod token_generator_atomic
{
    use std::{ops::RangeInclusive, sync::atomic::{AtomicUsize, Ordering}};

    use crate::sys::windows::tokens::{TOKEN_TYPE_MASK, TokenType};

    #[repr(transparent)]
    #[derive(Debug)]
    pub struct TokenGenInner(AtomicUsize);

    impl TokenGenInner
    {
        pub const
        fn new(token_type: usize) -> Self
        {
            return Self( AtomicUsize::new(token_type) );
        }

        pub
        fn next_mapped(&self, token_type: TokenType, range: RangeInclusive<usize>) -> usize
        {
            let mut last = self.0.load(Ordering::Relaxed);
            
            loop 
            {
                let next_token = 
                    match last.checked_add(1) 
                    {
                        Some(id) => id,
                        None =>
                        {
                            // cover 0xFFFF+1
                            panic!("exhausted!");
                        }
                    };

        
                // check that we are in range
                if range.contains(&last) == false
                {
                    panic!("exhausted range next: {:X} last: {:X} {:X} {:X}", next_token, last, next_token & TOKEN_TYPE_MASK, token_type.0);
                }

                let Err(new_token) = 
                    self.0.compare_exchange_weak(last, next_token, Ordering::SeqCst, Ordering::Relaxed)
                else { return last };

                last = new_token;
            }
        }

        pub
        fn next(&self) -> usize
        {
            let mut last = self.0.load(Ordering::Relaxed);
            
            loop 
            {
                let next_token = 
                    match last.checked_add(1) 
                    {
                        Some(id) => id,
                        None =>
                        {
                            // cover 0xFFFF+1
                            panic!("exhausted!");
                        }
                    };

                let Err(new_token) = 
                    self.0.compare_exchange_weak(last, next_token, Ordering::SeqCst, Ordering::Relaxed)
                else { return last };

                last = new_token;
            }
        }
    }
}

/// A realization of the token generator using mutex in case if atomics are not available.
#[cfg(not(target_has_atomic = "ptr"))]
pub mod token_generator_mutex
{
    use std::{ops::RangeInclusive, sync::Mutex};

    use crate::sys::windows::tokens::{TOKEN_TYPE_MASK, TokenType};

    #[repr(transparent)]
    #[derive(Debug)]
    pub struct TokenGenInner(Mutex<usize>);

    impl TokenGenInner
    {
        pub const 
        fn new(token_type: usize) -> Self
        {
            return Self( Mutex::new(token_type) );
        }

        pub
        fn next_mapped(&self, token_type: TokenType, range: RangeInclusive<usize>) -> usize
        {
            let mut lock = 
                self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);

            let cur_token = *lock;

            let next_token = 
                match cur_token.checked_add(1) 
                {
                    Some(token) =>
                    { 
                        // check that we are in range
                        if range.contains(&cur_token) == false
                        {
                            panic!("exhausted range next: {:X} last: {:X} {:X} {:X}", token, cur_token, token & TOKEN_TYPE_MASK, token_type.0);
                        }

                        token
                    },
                    None =>
                    {
                        // cover 0xFFFF+1
                        panic!("exhausted!");
                    }
                };

            *lock = next_token;

            return cur_token;
        }

        pub 
        fn next(&self, token_type: TokenType) -> usize
        {
            let mut lock = 
                self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);

            let cur_token = *lock;

            let next_token = 
                match cur_token.checked_add(1) 
                {
                    Some(token) =>
                    { 
                        // check that we are in range
                        if token & TOKEN_TYPE_MASK != token_type.0
                        {
                            panic!("exhausted range {}", token);
                        }

                        token
                    },
                    None =>
                    {
                        // cover 0xFFFF+1
                        panic!("exhausted!");
                    }
                };

            *lock = next_token;

            return cur_token;
        }
    }
}



use crate::Token;

#[cfg(target_has_atomic = "ptr")]
use self::token_generator_atomic::*;

#[cfg(not(target_has_atomic = "ptr"))]
use self::token_generator_mutex::*;

/// A `marker` which is used for token type declaration.
pub trait TokenSelector: Clone + Copy + fmt::Debug
{
    /// A code which indicates the owner of the token.
    const TOKEN_TYPE: TokenType;

    /// A mapped MSB bits.
    const TOKEN_TYPE_RANGE: RangeInclusive<usize>;

    /// New token.
    fn new(token: usize) -> Self where Self: Sized;

    /// Copies token from ref
    fn get_token(&self) -> Token;

    /// Consumes and returns token
    fn into_inner(self) -> Token;
}

/// A helper struct which indicates that the value is a token type.
#[repr(transparent)]
#[derive(Clone, Copy, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct TokenType(usize);

impl PartialEq<usize> for TokenType
{
    fn eq(&self, other: &usize) -> bool 
    {
        return self.0 == *other;
    }
}

impl TokenType
{
    pub const BIT_MASKING_OFFSET: usize = 3;

    /// Converts the `type` number of the token owner into the mapped value.
    const 
    fn derive(code: usize) -> Self
    {
        return Self(code << ((std::mem::size_of::<usize>() * 8) - Self::BIT_MASKING_OFFSET));
    }
}

/// Assigned to AFD type. 0..=2
const AFD_TOKEN: usize = 0x0;
const AFD_TOKEN_WIDTH: usize = 0x2;

/// Assigned to PIPE type 3..=5
const PIPE_TOKEN: usize = 0x3;
const PIPE_TOKEN_WIDTH: usize = 0x2;

/// Assigned to event type 6
const EVENT_TOKEN: usize = 0x6;
const EVENT_TOKEN_WIDTH: usize = 0;

/// Assigned to waker and rest 7
const WAKER_TOKEN: usize = 0x7;
const WAKER_TOKEN_WIDTH: usize = 0;

#[cfg(test)]
impl TokenSelector for usize
{
    const TOKEN_TYPE: TokenType = TokenType::derive(0);

    const TOKEN_TYPE_RANGE: RangeInclusive<usize> = (0..=0);

    fn new(token: usize) -> Self where Self: Sized 
    {
        token
    }

    fn get_token(&self) -> Token 
    {
        Token(*self)
    }

    fn into_inner(self) -> Token 
    {
        Token(self)
    }
}

/// Default (init)
#[derive(Debug, Clone, Copy)]
pub struct TokenDefault;

impl TokenSelector for TokenDefault
{
    const TOKEN_TYPE: TokenType = TokenType::derive(0);

    const TOKEN_TYPE_RANGE: RangeInclusive<usize> = (0..=0);

    fn new(_token: usize) -> Self where Self: Sized
    {
        return Self;
    }

    fn get_token(&self) -> Token
    {
        return Token(0);
    }

    fn into_inner(self) -> Token
    {
        return Token(0);
    }
}

/// AFD
#[derive(Debug, Clone, Copy)]
pub struct TokenAfd(Token);

impl TokenSelector for TokenAfd
{
    const TOKEN_TYPE: TokenType = TokenType::derive(AFD_TOKEN);

    const TOKEN_TYPE_RANGE: RangeInclusive<usize> = 
        (Self::TOKEN_TYPE.0..=TokenType::derive(AFD_TOKEN + AFD_TOKEN_WIDTH).0 | !TOKEN_TYPE_MASK);

    fn new(token: usize) -> Self where Self: Sized
    {
        return Self(Token(token));
    }

    fn get_token(&self) -> Token
    {
        return self.0;
    }

    fn into_inner(self) -> Token
    {
        return self.0;
    }
}

impl TokenAfd
{
    pub const 
    fn def() -> Self
    {
        return Self(Token(0));
    }
}

/// PIPE
#[derive(Debug, Clone, Copy)]
pub struct TokenPipe(Token);

impl TokenSelector for TokenPipe
{
    const TOKEN_TYPE: TokenType = TokenType::derive(PIPE_TOKEN);

    const TOKEN_TYPE_RANGE: RangeInclusive<usize> = 
        (Self::TOKEN_TYPE.0..=TokenType::derive(PIPE_TOKEN + PIPE_TOKEN_WIDTH).0 | !TOKEN_TYPE_MASK);
        

    fn new(token: usize) -> Self where Self: Sized
    {
        return Self(Token(token));
    }

    fn get_token(&self) -> Token
    {
        return self.0;
    }

    fn into_inner(self) -> Token
    {
        return self.0;
    }
}

impl TokenPipe
{
    pub const 
    fn def() -> Self
    {
        return Self(Token(0));
    }
}

/// EVENT
#[derive(Debug, Clone, Copy)]
pub struct TokenEvent(Token);

impl TokenSelector for TokenEvent
{
    const TOKEN_TYPE: TokenType = TokenType::derive(EVENT_TOKEN);

    const TOKEN_TYPE_RANGE: RangeInclusive<usize> = 
        (Self::TOKEN_TYPE.0..=TokenType::derive(EVENT_TOKEN + EVENT_TOKEN_WIDTH).0 | !TOKEN_TYPE_MASK);

    fn new(token: usize) -> Self where Self: Sized
    {
        return Self(Token(token));
    }

    fn get_token(&self) -> Token
    {
        return self.0;
    }

    fn into_inner(self) -> Token
    {
        return self.0;
    }
}

impl TokenEvent
{
    pub const 
    fn def() -> Self
    {
        return Self(Token(0));
    }
}

/// Intenral ID
#[cfg(debug_assertions)]
#[derive(Debug, Clone, Copy)]
pub struct WakerTokenId(Token);

impl WakerTokenId
{
    pub const 
    fn def() -> Self
    {
        return Self(Token(0));
    }
}

#[cfg(debug_assertions)]
impl TokenSelector for WakerTokenId
{
    const TOKEN_TYPE: TokenType = TokenType::derive(WAKER_TOKEN);

    const TOKEN_TYPE_RANGE: RangeInclusive<usize> = 
        (Self::TOKEN_TYPE.0..=TokenType::derive(WAKER_TOKEN + WAKER_TOKEN_WIDTH).0 | !TOKEN_TYPE_MASK);

    fn new(token: usize) -> Self where Self: Sized
    {
        return Self(Token(token));
    }

    fn get_token(&self) -> Token
    {
        return self.0;
    }

    fn into_inner(self) -> Token
    {
        return self.0;
    }
}

/// Mapped tokens generator.
#[repr(transparent)]
#[derive(Debug)]
pub struct TokenGenerator<TYPE: TokenSelector>(TokenGenInner, PhantomData<TYPE>);

impl<TYPE: TokenSelector>  TokenGenerator<TYPE>
{
    /// Creates new instance. Const.
    pub(super) const 
    fn new() -> Self
    {
        return Self( TokenGenInner::new(TYPE::TOKEN_TYPE.0), PhantomData );
    }

    #[cfg(test)]
    fn new_manual(offset: usize) -> Self
    {
        return Self( TokenGenInner::new(TYPE::TOKEN_TYPE.0 + offset), PhantomData );
    }

    /// Issues next token. MT-Safe.
    pub(super) 
    fn next(&self) -> TYPE
    {
        return TYPE::new(self.0.next_mapped(TYPE::TOKEN_TYPE, TYPE::TOKEN_TYPE_RANGE));
    }
}



/// Mask 0xF000 ... 0000
const TOKEN_TYPE_MASK: usize = (1 << ((std::mem::size_of::<usize>() * 8) - 8)) * 224;//240;


/// Decodes the mapped token.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DecocdedToken(usize);

impl From<usize> for DecocdedToken
{
    fn from(value: usize) -> Self 
    {
        return Self(value);
    }
}

impl<T: TokenSelector> PartialEq<T> for DecocdedToken
{
    fn eq(&self, _other: &T) -> bool 
    {
        return T::TOKEN_TYPE == self.decode_type(); 
    }
}

impl fmt::Display for DecocdedToken
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result 
    {
        write!(f, "raw: {:X}, type: {:X}, token: {:X}", self.0, self.decode_type(), self.decode_token())
    }
}

impl DecocdedToken
{
    /// Returns the token type shifter to the most right i.e 0x8000... to 0x0008
    #[allow(dead_code)]
    #[inline]
    pub(super)  
    fn get_source_code(&self) -> usize
    {
        return self.decode_type() >> ((std::mem::size_of::<usize>() * 8) - TokenType::BIT_MASKING_OFFSET) as usize;
    } 

    /// Returns the type of the mapped token.
    #[inline]
    pub(super)  
    fn decode_type(&self) -> usize
    {
        return self.0 & TOKEN_TYPE_MASK;
    }  

    /// Returns the number.
    #[inline]
    pub(super) 
    fn decode_token(&self) -> usize
    {
        return self.0 & !TOKEN_TYPE_MASK
    }
    
    /// Retuns `raw` value.
    #[inline]
    pub(super) 
    fn get_raw(&self) -> Token
    {
        return Token(self.0);
    }
}

/*
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct EncodedToken<TYPE: TokenSelector>(usize, PhantomData<TYPE>);

impl<TYPE: TokenSelector> From<Token> for EncodedToken<TYPE>
{
    fn from(value: Token) -> Self 
    {
        return Self(value.0 & TYPE::TOKEN_TYPE.0, PhantomData);
    }
}


impl<TYPE: TokenSelector> From<EncodedToken<TYPE>> for Token
{
    fn from(value: EncodedToken<TYPE>) -> Self 
    {
        return Token(value.0);
    }
}

impl<TYPE: TokenSelector> EncodedToken<TYPE>
{
    pub(super) const 
    fn static_binding(token: usize) -> Self
    {
        assert!(token < TOKEN_TYPE_BIT);

        return Self( TYPE::TOKEN_TYPE.0 + token, PhantomData );
    }
}
    */

#[cfg(test)]
mod tests
{
    use crate::sys::windows::tokens::
    {
        AFD_TOKEN, 
        TokenAfd, 
        TokenPipe, 
        TokenEvent, 
        WakerTokenId, 
        DecocdedToken, 
        EVENT_TOKEN, 
        PIPE_TOKEN, 
        TOKEN_TYPE_MASK, 
        TokenGenerator, 
        TokenSelector, 
        TokenType
    };

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn test0_token_type_mask()
    {
        assert_eq!(TOKEN_TYPE_MASK, 0xE000_0000_0000_0000);
        
        assert_eq!(TokenAfd::TOKEN_TYPE.0, 0x0000_0000_0000_0000);
        assert_eq!(TokenAfd::TOKEN_TYPE_RANGE, 0x0000_0000_0000_0000..=0x5FFF_FFFF_FFFF_FFFF);

        assert_eq!(TokenPipe::TOKEN_TYPE.0, 0x6000_0000_0000_0000);
        assert_eq!(TokenPipe::TOKEN_TYPE_RANGE, 0x6000_0000_0000_0000..=0xBFFF_FFFF_FFFF_FFFF);
        
        assert_eq!(TokenEvent::TOKEN_TYPE.0, 0xC000_0000_0000_0000);
        assert_eq!(TokenEvent::TOKEN_TYPE_RANGE, 0xC000_0000_0000_0000..=0xDFFF_FFFF_FFFF_FFFF);

        assert_eq!(WakerTokenId::TOKEN_TYPE.0, 0xE000_0000_0000_0000);
        assert_eq!(WakerTokenId::TOKEN_TYPE_RANGE, 0xE000_0000_0000_0000..=0xFFFF_FFFF_FFFF_FFFF);
    }

    #[cfg(target_pointer_width = "32")]
    #[test]
    fn test0_token_type_mask()
    {
        assert_eq!(TOKEN_TYPE_MASK, 0xF000_0000);

        assert_eq!(TokenAfd::TOKEN_TYPE.0, 0x0000_0000);
        assert_eq!(TokenAfd::TOKEN_TYPE_RANGE, 0x0000_0000..=0x5FFF_FFFF);

        assert_eq!(TokenPipe::TOKEN_TYPE.0, 0x6000_0000);
        assert_eq!(TokenPipe::TOKEN_TYPE_RANGE, 0x6000_0000..=0xBFFF_FFFF);
        
        assert_eq!(TokenEvent::TOKEN_TYPE.0, 0xC000_0000);
        assert_eq!(TokenEvent::TOKEN_TYPE_RANGE, 0xC000_0000..=0xDFFF_FFFF);

        assert_eq!(WakerTokenId::TOKEN_TYPE.0, 0xE000_0000);
        assert_eq!(WakerTokenId::TOKEN_TYPE_RANGE, 0xE000_0000..=0xFFFF_FFFF);
    }

    #[test]
    fn test00_derive()
    {
        println!("{:X}\n{:X}", TokenType::derive(0xf).0, TOKEN_TYPE_MASK);

        assert_eq!(TokenType::derive(0xf).0, TOKEN_TYPE_MASK);
    }

    fn test_token_type_sub(tok_n: usize, token: usize)
    {
        let mut tt = TokenType::derive(tok_n);
        tt.0 += token;

        let val = DecocdedToken::from(tt.0);

        assert_eq!(val.decode_token(), token);
        assert_eq!(val.decode_type(), TokenType::derive(tok_n).0);
        assert_eq!(val.get_source_code(), tok_n);
    }

    #[test]
    fn test1_token_type()
    {
        test_token_type_sub(AFD_TOKEN, 0);
        test_token_type_sub(PIPE_TOKEN, 0);
        test_token_type_sub(EVENT_TOKEN, 0);
    }

    #[test]
    fn decoded_token()
    {        
        test_token_type_sub(5, 10);
    }

    #[test]
    fn test2_generator_ev_type()
    {
        let gen_tok = TokenGenerator::<TokenEvent>::new();

        let mut deriv = TokenEvent::TOKEN_TYPE.0;

        let token = gen_tok.next();
        assert_eq!(token.0.0, deriv);

        let token = gen_tok.next();
        deriv += 1;
        assert_eq!(token.0.0, deriv);
    }
 
    #[test]
    #[should_panic = "exhausted range next: E000000000000001 last: E000000000000000 E000000000000000 C000000000000000"]
    fn test3_generator_ev_type()
    {
        let gen_tok = 
            TokenGenerator::<TokenEvent>::new_manual((!TOKEN_TYPE_MASK) - 2 );
            
        let mut deriv = TokenEvent::TOKEN_TYPE.0 + ((!TOKEN_TYPE_MASK) - 2);

        let token = gen_tok.next();
        println!("{:X} {:X}", token.0.0, deriv);
        assert_eq!(token.0.0, deriv);

        let token = gen_tok.next();
        deriv += 1;
        println!("{:X} {:X}", token.0.0, deriv);
        assert_eq!(token.0.0, deriv);

        let token = gen_tok.next();
        deriv += 1;
        println!("{:X} {:X}", token.0.0, deriv);
        assert_eq!(token.0.0, deriv);

        let _token = gen_tok.next();
    }
}
