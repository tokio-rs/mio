use std::fmt;
use token::Token;
use handler::{ReadHint, DATAHINT, HUPHINT, ERRORHINT};


bitflags!(
          flags IoEventKind: uint {
          const IOREADABLE = 0x001,
          const IOWRITABLE = 0x002,
          const IOERROR    = 0x004,
          const IOHUPHINT  = 0x008,
          const IOHINTED   = 0x010,
          const IOONESHOT  = 0x020,
          const IOEDGE     = 0x040,
          const IOLEVEL    = 0x080,
          const IOALL      = 0x001 | 0x002 | 0x004 | 0x008 
          }
         )

impl fmt::Show for IoEventKind {
  fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
    let mut one = false;
    let flags = [
      (IOREADABLE, "IoReadable"),
      (IOWRITABLE, "IoWritable"),
      (IOERROR, "IoError"),
      (IOHUPHINT, "IoHupHint"),
      (IOHINTED, "IoHinted"),
      (IOEDGE, "IoEdgeTriggered")];

    for &(flag, msg) in flags.iter() {
      if self.contains(flag) {
        if one { try!(write!(fmt, " | ")) }
        try!(write!(fmt, "{}", msg));

        one = true
      }
    }

    Ok(())
  }
}

#[deriving(Show)]
pub struct IoEventCtx {
kind: IoEventKind,
        token: Token
}

/// IoEventCtx represents the raw event upon which the OS-specific selector
/// operates. It takes an IoEventCtx when registering and re-registering event
/// interest for Tokens. It also returns IoEventCtx when calling handlers. 
/// An event can represent more than one kind (such as readable or writable) at a time.
///
/// These IoEventCtx objects are created by the OS-specific concrete
/// Selector when they have events to report.
impl IoEventCtx {
  /// Create a new IoEvent.
  pub fn new(kind: IoEventKind, token: Token) -> IoEventCtx {
    IoEventCtx {
        kind: kind, 
        token: token
    }
  }

  pub fn token(&self) -> Token {
    self.token
  }

  /// Return an optional hint for a readable IO handle. Currently,
  /// this method supports the HupHint, which indicates that the
  /// kernel reported that the remote side hung up. This allows a
  /// consumer to avoid reading in order to discover the hangup.
  pub fn read_hint(&self) -> ReadHint {
    let mut hint = ReadHint::empty();

    // The backend doesn't support hinting
    if !self.kind.contains(IOHINTED) {
      return hint;
    }

    if self.kind.contains(IOHUPHINT) {
      hint = hint | HUPHINT
    }

    if self.kind.contains(IOREADABLE) {
      hint = hint | DATAHINT
    }

    if self.kind.contains(IOERROR) {
      hint = hint | ERRORHINT
    }

    hint
  }

  /// This event indicated that the IO handle is now readable
  pub fn is_readable(&self) -> bool {
    self.kind.contains(IOREADABLE) || self.kind.contains(IOHUPHINT)
  }

  /// This event indicated that the IO handle is now writable
  pub fn is_writable(&self) -> bool {
    self.kind.contains(IOWRITABLE)
  }

  /// This event indicated that the IO handle had an error
  pub fn is_error(&self) -> bool {
    self.kind.contains(IOERROR)
  }

  pub fn is_hangup(&self) -> bool {
    self.kind.contains(IOHUPHINT)
  }

  pub fn is_edge_triggered(&self) -> bool {
    self.kind.contains(IOEDGE)
  }

  /// This event indicated that the IO handle is now readable
  pub fn set_readable(&mut self, flag: bool) -> &IoEventCtx {
    match flag {
      true => self.kind.insert(IOREADABLE),
      false => self.kind.remove(IOREADABLE)
    }
    return self;
  }

  /// This event indicated that the IO handle is now writable
  pub fn set_writable(&mut self, flag: bool) -> &IoEventCtx { 
    match flag {
      true => self.kind.insert(IOWRITABLE),
      false => self.kind.remove(IOWRITABLE)
    }
    return self;
  }

  /// This event indicated that the IO handle had an error
  pub fn set_error(&mut self, flag: bool) -> &IoEventCtx {
    match flag {
      true => self.kind.insert(IOERROR),
      false => self.kind.remove(IOERROR)
    }
    return self;
  }

  pub fn set_hangup(&mut self, flag: bool) -> &IoEventCtx {
    match flag {
      true => self.kind.insert(IOHUPHINT),
      false => self.kind.remove(IOHUPHINT)
    }
    return self;
  }

  pub fn set_edge_triggered(&mut self, flag: bool) -> &IoEventCtx {
    match flag {
      true => self.kind.insert(IOEDGE),
      false => self.kind.remove(IOEDGE)
    }
    return self;
  }

  pub fn set_all(&mut self, flag: bool) -> &IoEventCtx {
    match flag {
      true => self.kind.insert(IOALL),
      false => self.kind.remove(IOALL)
    }
    return self;
  }
}

