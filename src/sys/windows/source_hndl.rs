use std::
{
    cell::OnceCell, 
    cmp::Ordering, 
    collections::{BTreeSet}, 
    io::{self, ErrorKind}, 
    os::
    {
        raw::c_void, 
        windows::io::{AsHandle, AsRawHandle, BorrowedHandle, OwnedHandle, RawHandle}
    }, 
    ptr::null_mut, 
    sync::Arc, 
    usize
};

use windows_sys::Win32::{Storage::FileSystem::{FILE_TYPE_UNKNOWN, GetFileType}};

use crate::
{
    Interest, 
    Registry, 
    Token, 
    event::Source, 
    sys::
    {
        SelectorInner, 
        event::INT_FLAG_WIN_EVENT, 
        windows::ffi::{self, IO_CANCELLED_ERROR, ffi_nt_create_wait_completion_packet}
    }
};

use super::ffi::HANDLE;

/// A wait completion packet.
/// 
/// It contains the `event handle` and a `completion handle`.
/// 
/// The `completion handle` is owned by this instance and auto 
/// closed when instance goes out of scope. The `event handle`
/// is closed by the owner.
/// 
/// The `token` is assigned outside and must be unique.
#[derive(Debug)]
pub(crate) struct SourceCompPack
{
    /// Event
    event_hndl: HANDLE,

    /// Completion packet which is associated to `completion port` to which
    /// the `event_hndl` is assigned.
    comp_hndl: OwnedHandle,

    /// Token which was assigned outside. Unique.
    /// Used for binary search ans sorking in ascending order.
    token: Token
}

unsafe impl Send for SourceCompPack {}

impl Eq for SourceCompPack {}

impl PartialEq for SourceCompPack
{
    fn eq(&self, other: &Self) -> bool 
    {
        return self.event_hndl == other.event_hndl;
    }
}

impl PartialEq<Token> for &SourceCompPack
{
    fn eq(&self, other: &Token) -> bool 
    {
        return self.token == *other;
    }
}

impl Ord for SourceCompPack
{
    fn cmp(&self, other: &Self) -> Ordering 
    {
        return self.token.cmp(&other.token);
    }
}

impl PartialOrd for SourceCompPack
{
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> 
    {
        return Some(self.cmp(other));
    }
}

impl SourceCompPack
{
    /// Creates new instance. The `ev_hndl` is borrowed to make sure that the
    /// provided argument originates from the source.
    pub(crate) 
    fn new(ev_hndl: BorrowedHandle<'_>, token: Token) -> io::Result<Self>
    {
        let wcp_oh = ffi_nt_create_wait_completion_packet()?;

        return Ok(
            Self
            {
                event_hndl: 
                    ev_hndl.as_raw_handle(),
                comp_hndl: 
                    wcp_oh,
                token: 
                    token,
            }
        );
    }

    pub(crate) 
    fn set_token(&mut self, token: Token)
    {
        self.token = token;
    }

    /// A wrapper around the [ffi::NtAssociateWaitCompletionPacket]. 
    /// 
    /// `iocp_handle` is a valid [RawHandle] to the initial `IoCompletionPort`.
    /// 
    /// # Returns
    /// 
    /// An [Result::Err] is returned:
    /// 
    /// * [ffi::NTSTATUS] is returned
    pub(crate) 
    fn associate_wait_completion_packet(&self, iocp_handle: RawHandle) -> io::Result<()>
    {
        unsafe
        {
            ffi::
                NtAssociateWaitCompletionPacket(
                    self.comp_hndl.as_raw_handle(), 
                    iocp_handle, 
                    self.event_hndl, 
                    self.token.0 as *mut c_void, // OVERLAPED_ENTRY.lpCompletionKey
                    null_mut(), // OVERLAPED_ENTRY.lpOverlapped
                    0, //OVERLAPED_ENTRY.Internal
                    INT_FLAG_WIN_EVENT as usize, //OVERLAPED_ENTRY.dwNumberOfBytesTransfered
                    null_mut()
                )
                .into_result()?
        };

        return Ok(());
    }

    /// A inverse of the [Self::associate_wait_completion_packet] which disassociates the 
    /// `compl handle` from `port completion`. The error [IO_CANCELLED_ERROR] is masked, so calling
    /// it on previously deassiciated `handle` will not produce this error.
    /// 
    /// # Arguments
    /// 
    /// * `rem_sig_packet` is a
    /// > a boolean which determines whether this function will attempt to cancel a packet 
    /// > from an IO completion object queue.
    /// 
    /// # Returns
    /// 
    /// An [Result::Err] is returned:
    /// 
    /// * [ffi::NTSTATUS] is returned except [IO_CANCELLED_ERROR]
    pub(crate)
    fn deassociate_wait_completion_packet(&self, rem_sig_packet: bool) -> io::Result<()>
    {
        let res = 
            unsafe
            {
                ffi::
                    NtCancelWaitCompletionPacket(self.comp_hndl.as_raw_handle(), rem_sig_packet.into())
            };
        
        if res.into_hresult() == IO_CANCELLED_ERROR
        {
            // ignore double deassociation
            return Ok(());
        }

        return res.into_result();
    }
}

/// A wait completion packet storage.
/// 
/// Sorted in ascending order, because the `binary search` is used 
/// to search on the list. No duplicates. The values are sorted by
/// the `token`, not by handlers!
/// 
/// `token` must be an uniq value otherwise the function which 
/// relies on searching by token will fail. 
/// 
/// **Do NOT push unsorted!**
/// 
/// This must never be clonned!
/// 
/// Not MT-safe. A mutex should be used to guard access.
#[derive(Debug, Default)]
pub(crate) struct SourceCompPacks
{
    /// A list of WaitCompletionPacket associations.
    scp_list: Vec<SourceCompPack>,

    /// A `ev_hndl` duplicate detector.
    ev_hndl_dup: BTreeSet<usize>,
}

impl SourceCompPacks
{
    pub(crate)
    fn new() -> Self
    {
        return 
            Self
            { 
                scp_list: Vec::with_capacity(10),
                ev_hndl_dup: BTreeSet::new(),
            };
    }

    /// Inserts the `scp` to the list. The `scp` token must be 
    /// unique because it is used as a key for binary_searching and
    /// sorting.
    /// 
    /// This function also checks if `ev_hndl` of `scp` have been already
    /// inserted.
    /// 
    /// # Arguments 
    /// 
    /// * `scp` - a [SourceCompPack] structure.
    /// 
    /// # Returns
    /// 
    /// An [Result::Err] is returned:
    /// 
    /// * [ErrorKind::NotFound] - in case if record with `event handle` is already on
    /// the list or if token of the [SourceCompPack] was found. The error description 
    /// is a [String] type.
    /// 
    /// Otherwise, a mutable reference to stored [SourceCompPack] is returned.
    pub(crate) 
    fn insert_sorted(&mut self, scp: SourceCompPack) -> io::Result<&mut SourceCompPack>
    {
        let res = 
            self
                .scp_list
                .binary_search_by(|l_scp| 
                    l_scp.cmp(&scp) // searches by token
                );

        match res
        {
            Ok(_index) => 
                return Err( 
                    io::Error::new(
                        ErrorKind::AlreadyExists, 
                        format!("token is on the list: {}", scp.token.0)
                    ) 
                ),
            Err(index) => 
            {
                // check for token duplicates
                if let Some(_) = self.ev_hndl_dup.get(&(scp.event_hndl as usize))
                {
                    return Err( 
                        io::Error::new(
                            ErrorKind::AlreadyExists, 
                            format!("ev_handl is on the list: {}", scp.event_hndl as usize)
                        ) 
                    );
                }

                self.scp_list.insert(index, scp);

                return Ok( self.scp_list.get_mut(index).unwrap() );
            }
        }
    }


    /// Searches on the list using binary search.
    #[inline]
    fn binary_search_token_internal(&self, token: Token) -> io::Result<usize>
    {
        return 
            self
                .scp_list
                .binary_search_by_key(&token, |scp| scp.token)
                .map_err(|_e|
                    io::Error::new(ErrorKind::NotFound, format!("token: {} not found!", token.0))
                );
    }

    /// Searches for the record by the `token` using binary search.
    /// 
    /// # Returns
    /// 
    /// An [Result::Err] is returned:
    /// 
    /// * [ErrorKind::NotFound] - in case if record with `token` does not exist. 
    /// The error description is a [String] type.
    /// 
    /// Otherwise, the mutable reference to [SourceCompPack] is returned.
    /// V opačnom prípade sa vráti meniteľný odkaz na [SourceCompPack].
    pub(crate) 
    fn search(&mut self, token: Token) -> io::Result<&mut SourceCompPack>
    {
        return 
            self
                .binary_search_token_internal(token)
                .map(|scp_index|
                    self.scp_list.get_mut(scp_index).unwrap()
                );
    }

    /// Removes the record by the `event` handle!
    /// 
    /// # Returns
    /// 
    /// An [Result::Err] is returned:
    /// 
    /// * [ErrorKind::NotFound] - in case if record with `token` does not exist. 
    /// The error description is a [String] type.
    /// 
    /// Otherwise, the inner type contains [SourceCompPack]. See description for this
    /// structure.
    pub(crate) 
    fn remove(&mut self, token: Token) -> io::Result<SourceCompPack>
    {
        return 
            self
                .binary_search_token_internal(token)
                .map(|scp_index|
                    {
                        let scp = self.scp_list.remove(scp_index);

                        // remove the token from dup finder
                        let _ = self.ev_hndl_dup.remove(&(scp.event_hndl as usize));

                        scp
                    }
                );
    }

    /*pub(crate) 
    fn re_associate_wait_comp_pack(&self, iocp_handle: RawHandle) -> io::Result<()>
    {
        return 
            self
                .scp_list
                .iter()
                .map(|v| 
                    v.associate_wait_completion_packet(iocp_handle)
                )
                .collect();
    }*/

    /// Performs a `NtAssociateWaitCompletionPacket` for the [SourceCompPack] by [Token] which was
    /// assigned. The reason (why `token` was used is this function is called during the event feeding).
    /// 
    /// It does NOT use a binary_search because it is not possible to resolve the `event` handle from
    /// `token`.
    /// 
    /// It is needed because:
    /// 
    /// > When the packet is picked up, the association must be reestablished by calling this function again. 
    /// > An error is returned if the wait completion packet is already in use for an association.
    /// 
    /// # Arguments
    /// 
    /// * `iocp_hanle` - a [RawHandle] to the initial `IoCompletionPort` which is polled.
    /// 
    /// * `token` - [Token] which identifies the instance.
    /// 
    /// # Returns
    /// 
    /// An [Result::Err] is returned:
    /// 
    /// * [ErrorKind::NotFound] - in case if record with `token` does not exist.
    /// The error description is a [String] type.
    /// 
    /// * a raw OS error with `NTSTATUS` code. No description is awailable.
    pub(crate) 
    fn re_associate_wait_comp_pack_by_token(&self, iocp_handle: RawHandle, token: Token) -> io::Result<()>
    {
        let scp = 
            self
                .binary_search_token_internal(token)
                .map(|scp_index|
                    self.scp_list.get(scp_index).unwrap()
                )?;

        return scp.associate_wait_completion_packet(iocp_handle);
    }
}

/// Adapter for the any `HNDL` structure which implements the
/// [AsHandle] to any `event` handle from the list below:
/// 
/// * CreateWaitableTimer
/// 
/// * CreateEvent
/// 
/// * or other where `handle` is an event object.
/// 
/// The structure provides a [Source] implementation and auto 
/// managment of the registration.
/// 
/// ```ignore
/// pub struct PrimitiveTimer
/// {
///     hndl_timer: OwnedHandle
/// }
/// 
/// impl AsHandle for PrimitiveTimer
/// {
///     fn as_handle(&self) -> std::os::windows::prelude::BorrowedHandle<'_> 
///     {
///         return self.hndl_timer.as_handle();
///     }
/// }
/// 
/// //...
/// 
/// let mut se_hndl_timer = SourceEventHndl::new(PrimitiveTimer::new("timer_1")).unwrap();
/// 
/// let mut poll = Poll::new().unwrap();
/// 
/// poll.registry().register(&mut se_hndl_timer, Token(1), Interest::READABLE).unwrap();
/// ```
/// 
/// The token of each instance must be unique! Also it is not possible to register the 
/// same handle with different `tokens`.
#[derive(Debug)]
pub struct SourceEventHndl<HNDL: AsHandle>
{
    /// A event based instance
    ev_source: OnceCell<HNDL>,

    /// A bind to selector.
    si_bind: Option<(Arc<SelectorInner>, Token)>,
}

impl<HNDL: AsHandle> AsHandle for SourceEventHndl<HNDL>
{
    fn as_handle(&self) -> BorrowedHandle<'_> 
    {
        return self.ev_source.get().unwrap().as_handle();
    }
}

impl<HNDL: AsHandle> AsRawHandle for SourceEventHndl<HNDL>
{
    fn as_raw_handle(&self) -> RawHandle 
    {
        return self.as_handle().as_raw_handle();
    }
}

impl<HNDL: AsHandle> Drop for SourceEventHndl<HNDL>
{
    fn drop(&mut self) 
    {
        let _ = self.unreg(); 

        let _ = self.ev_source.take();
    }
}

impl<HNDL: AsHandle> SourceEventHndl<HNDL>
{
    /// Initializes the instance for the `HNDL`.
    /// 
    /// # Returns
    /// 
    /// An [Result::Err] is returned:
    /// 
    /// * [ErrorKind::InvalidData] - is returned if the [GetFileType] returns other
    ///     than the type [FILE_TYPE_UNKNOWN].
    /// 
    /// Otherwise, the instance is returned.
    pub 
    fn new(hndl: HNDL) -> io::Result<Self>
    {
        // try to verify that this is an event
        if unsafe { GetFileType(hndl.as_handle().as_raw_handle()) } != FILE_TYPE_UNKNOWN
        {
            return Err(io::Error::new(ErrorKind::InvalidData, "handler is not event type"));
        }

        return Ok(
            Self
            {
                ev_source: OnceCell::from(hndl),
                //token: None,
                si_bind: None,
            }
        );
    }

    /// Returns the [Token] as inner value if instance was registered.
    pub 
    fn get_token(&self) -> Option<Token>
    {
        return self.si_bind.as_ref().map(|(_, token)| *token);
    }

    /// Internal function. Unregisteres the instance from the `completion_packs` list
    /// of the [SelectorInner] and removes the association.
    /// 
    /// Returns error produced by [SelectorInner::unregester_completion_pack].
    fn unreg(&mut self) -> io::Result<()>
    {
        return 
            if let Some((si, token)) = self.si_bind.take()
            {
                si
                    .unregister_completion_pack(token)
                    .map(|_| ())
            }
            else
            {
                Ok(())
            };
    }

    /// Attempts to consume the `self` and return the wrapped value.
    /// 
    /// The operation may fail if the `unregister` operation fails 
    /// for any reason. If current instance was unregistered prior to
    /// calling this function, should be completed without any errors.
    /// 
    /// # Returns
    /// 
    /// A tuple (Self, [Result::Err]) is returned:
    /// 
    /// * error produced by [Self::unreg].
    /// 
    /// Otherwise, the `HNDL` will be returned.
    pub 
    fn try_into_inner(mut self) -> Result<HNDL, (Self, io::Error)>
    {
        if let Err(e) = self.unreg()
        {
            return Err((self, e));
        }
        else
        {
            return Ok(self.ev_source.take().unwrap());
        }
    }

    /// Returns a reference to &HNDL.
    pub 
    fn inner(&self) -> &HNDL
    {
        return self.ev_source.get().unwrap();
    }

    /// Returns a mutable reference to &HNDL.
    pub 
    fn inner_mut(&mut self) -> &mut HNDL
    {
        return self.ev_source.get_mut().unwrap();
    }
}


impl<HNDL: AsHandle> Source for SourceEventHndl<HNDL>
{
    /// # Returns
    /// 
    /// Errors: 
    /// 
    /// * [ErrorKind::ResourceBusy] - if already registered, error description [String]
    /// 
    /// * [SourceCompPack::new] - errors
    /// 
    /// * [SelectorInner::regester_completion_pack] - error
    fn register(&mut self, registry: &Registry, token: Token, _interests: Interest) -> io::Result<()> 
    {
        let ev_source = self.ev_source.get().unwrap();

        // check if already binded
        if let Some((si, token)) = self.si_bind.as_ref()
        {
            return Err(
                io::Error::new(
                    ErrorKind::ResourceBusy, 
                    format!("hndl: {} token: {} already polled by {}", 
                        ev_source.as_handle().as_raw_handle() as usize, 
                        token.0,
                        si.cp.as_raw_handle() as usize)
                )
            )
        }

        // creating pair event:completion
        let scp = SourceCompPack::new(ev_source.as_handle(), token)?;

        // regester completion pack
        registry.selector().inner.regester_completion_pack(scp)?;

        // bind
        self.si_bind.replace((registry.selector().inner.clone(), token));

        return Ok(());
    }

    /// # Returns
    /// 
    /// Errors: 
    /// 
    /// * [ErrorKind::NotConnected] - if instance was not regestered previously, 
    ///     error description [String]
    /// 
    /// * [SelectorInner::update_completion_pack_token] errors
    /// 
    /// * [SelectorInner::regester_completion_pack] errors
    /// 
    /// * [SelectorInner::unregister_completion_pack] errors
    fn reregister(
        &mut self,
        registry: &Registry,
        token: Token,
        _interests: Interest,
    ) -> io::Result<()> 
    {
        let ev_source = self.ev_source.get().unwrap();

        if let Some((si, se_token)) = self.si_bind.take()
        {
            if registry.selector().same_port(&si.cp) == false
            {
                let mut scp = 
                    si.unregister_completion_pack(se_token)?;

                // update token just in case
                scp.set_token(token);

                // regester completion pack
                registry.selector().inner.regester_completion_pack(scp)?;
            }
            else if se_token != token
            {
                // update token in SourceCompPack on the SelectorInner side
                si.replace_completion_pack_token(se_token, token)?;
            }
            
            // restore 
            self.si_bind.replace((registry.selector().inner.clone(), token));

            // otherwise nothing has changed

            return Ok(());
        }
        else
        {
             return Err(
                io::Error::new(
                    ErrorKind::NotConnected, 
                    format!("hndl: {} not regestred with Registry", 
                        ev_source.as_handle().as_raw_handle() as usize)
                )
            )
        }
    }

    /// # Returns
    /// 
    /// Errors: 
    /// 
    /// * [ErrorKind::CrossesDevices] - if current instance is on different port, 
    ///     error description [String].
    /// 
    /// * [ErrorKind::NotConnected] - if instance was not regestered previously, 
    ///     error description [String]
    /// 
    /// * [SelectorInner::unregister_completion_pack] - errors
    fn deregister(&mut self, registry: &Registry) -> io::Result<()> 
    {
        if let Some((si, se_token)) = self.si_bind.take()
        {
            if registry.selector().same_port(&si.cp) == false
            {
                return Err(
                    io::Error::new(
                        ErrorKind::CrossesDevices, 
                        format!("hndl: {} token: {} is on different port", 
                            self.ev_source.get().unwrap().as_handle().as_raw_handle() as usize,
                            se_token.0)
                    )
                )
            }

            // deregister the instance
            let _ = 
                registry
                    .selector()
                    .inner
                    .unregister_completion_pack(se_token)?;
        }
        else
        {
            return Err(
                io::Error::new(
                    ErrorKind::NotConnected, 
                    format!("hndl: {} not regestred with Registry", 
                        self.ev_source.get().unwrap().as_handle().as_raw_handle() as usize)
                )
            )
        }

        return Ok(());
    }
}

