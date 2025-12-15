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
 | N/a      | 0x0000_0000_0000_0000-0x7FFF_FFFF_FFFF_FFFF | 0-7 x | X | X | X | X | X | X | X |
 | AFD      | 0x8000_0000_0000_0000-0x9FFF_FFFF_FFFF_FFFF | 8-9 x | X | X | X | X | X | X | X |
 | Pipe     | 0xA000_0000_0000_0000-0xBFFF_FFFF_FFFF_FFFF | A-B x | X | X | X | X | X | X | X |
 | Event    | 0xC000_0000_0000_0000-0xCFFF_FFFF_FFFF_FFFF | C   x | X | X | X | X | X | X | X |
 | Internal | 0xD000_0000_0000_0000-0xDFFF_FFFF_FFFF_FFFF | D   x | X | X | X | X | X | X | X |
 | Spare    | 0xE000_0000_0000_0000-0xFFFF_FFFF_FFFF_FFFF | E-F x | x | x | x | x | x | x | x |
 +----------+---------------------------------------------+-------+---+---+---+---+---+---+---+
 * ```
 * 
 * 0000_0000_0000_0000 - 7FFF_FFFF_FFFF_FFFF = 9 223 372 036 854 775 807
 * 8000_0000_0000_0000 - 9FFF_FFFF_FFFF_FFFF = 2 305 843 009 213 693 951
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
    pub(super) struct TokenGenInner(AtomicUsize);

    impl TokenGenInner
    {
        pub(super) const
        fn new(token_type: usize) -> Self
        {
            return Self( AtomicUsize::new(token_type) );
        }

        pub(super)
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
                if range.contains(&(next_token & TOKEN_TYPE_MASK)) == false
                {
                    panic!("exhausted range {:X} {:X} {:X}", next_token, next_token & TOKEN_TYPE_MASK, token_type.0);
                }

                let Err(new_token) = 
                    self.0.compare_exchange_weak(last, next_token, Ordering::SeqCst, Ordering::Relaxed)
                else { return last };

                last = new_token;
            }
        }

        pub(super)
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
    pub(super) struct TokenGenInner(Mutex<usize>);

    impl TokenGenInner
    {
        pub(super) const 
        fn new(token_type: usize) -> Self
        {
            return Self( Mutex::new(token_type) );
        }

        pub(super)
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
                        if range.contains(&(token & TOKEN_TYPE_MASK)) == false
                        {
                            panic!("exhausted range {:X} {:X} {:X}", token, token & TOKEN_TYPE_MASK, token_type.0);
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

        pub(super)
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
pub(super) trait TokenSelector: Clone + Copy + fmt::Debug
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
pub(super) struct TokenType(usize);

impl PartialEq<usize> for TokenType
{
    fn eq(&self, other: &usize) -> bool 
    {
        return self.0 == *other;
    }
}

impl TokenType
{
    /// Converts the `type` number of the token owner into the mapped value.
    const 
    fn derive(code: usize) -> Self
    {
        return Self(code << ((std::mem::size_of::<usize>() * 8) - 4));
    }
}

/// Assigned to AFD type.
const AFD_TOKEN: usize = 0x8;

/// Assigned to PIPE type
const PIPE_TOKEN: usize = 0xA;

/// Assigned to event type
const EVENT_TOKEN: usize = 0xC;

const WAKER_TOKEN: usize = 0xD;

/// Default (init)
#[derive(Debug, Clone, Copy)]
pub(super) struct TokenDefault;

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
pub(super) struct TokenAfd(Token);

impl TokenSelector for TokenAfd
{
    const TOKEN_TYPE: TokenType = TokenType::derive(AFD_TOKEN);

    const TOKEN_TYPE_RANGE: RangeInclusive<usize> = 
        (Self::TOKEN_TYPE.0..=TokenType::derive(AFD_TOKEN + 1).0 | !TOKEN_TYPE_MASK);

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
pub(super) struct TokenPipe(Token);

impl TokenSelector for TokenPipe
{
    const TOKEN_TYPE: TokenType = TokenType::derive(PIPE_TOKEN);

    const TOKEN_TYPE_RANGE: RangeInclusive<usize> = 
        (Self::TOKEN_TYPE.0..=TokenType::derive(PIPE_TOKEN + 1).0 | !TOKEN_TYPE_MASK);
        

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
pub(super) struct TokenEvent(Token);

impl TokenSelector for TokenEvent
{
    const TOKEN_TYPE: TokenType = TokenType::derive(EVENT_TOKEN);

    const TOKEN_TYPE_RANGE: RangeInclusive<usize> = 
        (Self::TOKEN_TYPE.0..=TokenType::derive(EVENT_TOKEN).0 | !TOKEN_TYPE_MASK);

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
pub(super) struct WakerTokenId(Token);

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
        (Self::TOKEN_TYPE.0..=TokenType::derive(WAKER_TOKEN).0 | !TOKEN_TYPE_MASK);

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
pub(super) struct TokenGenerator<TYPE: TokenSelector>(TokenGenInner, PhantomData<TYPE>);

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
const TOKEN_TYPE_MASK: usize = (1 << ((std::mem::size_of::<usize>() * 8) - 8)) * 240;

/// MSB bit
const TOKEN_TYPE_BIT: usize = (1 << ((std::mem::size_of::<usize>() * 8) - 8)) * 128;

/// Decodes the mapped token.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct DecocdedToken(usize);

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
        return self.decode_type() >> ((std::mem::size_of::<usize>() * 8) - 4) as usize;
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
        assert_eq!(TOKEN_TYPE_MASK, 0xF000_0000_0000_0000);

        assert_eq!(TokenAfd::TOKEN_TYPE.0, 0x8000_0000_0000_0000);
        assert_eq!(TokenAfd::TOKEN_TYPE_RANGE, 0x8000_0000_0000_0000..=0x9FFF_FFFF_FFFF_FFFF);

        assert_eq!(TokenPipe::TOKEN_TYPE.0, 0xA000_0000_0000_0000);
        assert_eq!(TokenPipe::TOKEN_TYPE_RANGE, 0xA000_0000_0000_0000..=0xBFFF_FFFF_FFFF_FFFF);
        
        assert_eq!(TokenEvent::TOKEN_TYPE.0, 0xC000_0000_0000_0000);
        assert_eq!(TokenEvent::TOKEN_TYPE_RANGE, 0xC000_0000_0000_0000..=0xCFFF_FFFF_FFFF_FFFF);

        assert_eq!(WakerTokenId::TOKEN_TYPE.0, 0xD000_0000_0000_0000);
        assert_eq!(WakerTokenId::TOKEN_TYPE_RANGE, 0xD000_0000_0000_0000..=0xDFFF_FFFF_FFFF_FFFF);
    }

    #[cfg(target_pointer_width = "32")]
    #[test]
    fn test0_token_type_mask()
    {
        assert_eq!(TOKEN_TYPE_MASK, 0xF000_0000);

        assert_eq!(TokenAfd::TOKEN_TYPE.0, 0x8000_0000);
        assert_eq!(TokenAfd::TOKEN_TYPE_RANGE, 0x8000_0000..=0x9FFF_FFFF);

        assert_eq!(TokenPipe::TOKEN_TYPE.0, 0xA000_0000);
        assert_eq!(TokenPipe::TOKEN_TYPE_RANGE, 0xA000_0000..=0xBFFF_FFFF);
        
        assert_eq!(TokenEvent::TOKEN_TYPE.0, 0xC000_0000);
        assert_eq!(TokenEvent::TOKEN_TYPE_RANGE, 0xC000_0000..=0xCFFF_FFFF);

        assert_eq!(WakerTokenId::TOKEN_TYPE.0, 0xD000_0000);
        assert_eq!(WakerTokenId::TOKEN_TYPE_RANGE, 0xD000_0000..=0xDFFF_FFFF);
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
    #[should_panic = "exhausted range D000000000000000 D000000000000000 C000000000000000"]
    fn test3_generator_ev_type()
    {
        let gen_tok = 
            TokenGenerator::<TokenEvent>::new_manual((!TOKEN_TYPE_MASK) - 2 );
            
        let mut deriv = TokenEvent::TOKEN_TYPE.0 + ((!TOKEN_TYPE_MASK) - 2);

        let token = gen_tok.next();
        assert_eq!(token.0.0, deriv);

        let token = gen_tok.next();
        deriv += 1;
        assert_eq!(token.0.0, deriv);

        let _token = gen_tok.next();
    }
}
