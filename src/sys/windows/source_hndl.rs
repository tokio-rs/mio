use std::
{
    cell::OnceCell, collections::BTreeMap, io::{self, ErrorKind}, os::
    {
        raw::c_void, 
        windows::io::{AsHandle, AsRawHandle, BorrowedHandle, OwnedHandle, RawHandle}
    }, ptr::null_mut, sync::{Arc, Mutex, OnceLock, Weak}, usize
};

use windows_sys::Win32::{Storage::FileSystem::{FILE_TYPE_UNKNOWN, GetFileType}, System::IO::{OVERLAPPED_ENTRY}};

use crate::
{
    Interest, 
    Registry, 
    Token, 
    event::Source, 
    sys::
    {
        SelectorInner, windows::{Event, ffi::{self, IO_CANCELLED_ERROR, ffi_nt_create_wait_completion_packet}, iocp::CompletionStatus, tokens::{TokenEvent, TokenGenerator, TokenSelector}}
    }
};

use super::ffi::HANDLE;


static NEXT_TOKEN: TokenGenerator<TokenEvent> = TokenGenerator::new();

/// A wait completion packet.
/// 
/// It contains the `event handle` and a `completion handle`.
/// 
/// The `completion handle` is owned by this instance and auto 
/// closed when instance goes out of scope. The `event handle`
/// is closed by the owner.
/// 
/// `internal_token` is generated automatically. It is visible
/// for `selector`.
/// 
/// `user_token` is assigned by user and visible only yo user.
#[derive(Debug)]
pub(crate) struct SourceCompPack
{
    /// Event
    event_hndl: HANDLE,

    /// Completion packet which is associated to `completion port` to which
    /// the `event_hndl` is assigned.
    comp_hndl: OwnedHandle,

    /// Token which was AUTOMATICALLY assigned. Unique.
    /// Used for binary search ans sorking in ascending order.
    internal_token: TokenEvent,

    /// user assigned token
    user_token: Token,

    /// selector
    si: Arc<SelectorInner>,

    /// A weak reference to this stuct
    me: Weak<Mutex<SourceCompPack>>,
}

unsafe impl Sync for SourceCompPack {}

unsafe impl Send for SourceCompPack {}

impl Drop for SourceCompPack
{
    fn drop(&mut self) 
    {
        let _ = self.deassociate_wait_completion_packet(true);
    }
}

impl Eq for SourceCompPack {}

impl PartialEq for SourceCompPack
{
    fn eq(&self, other: &Self) -> bool 
    {
        return self.event_hndl == other.event_hndl;
    }
}

/*
impl PartialEq<Token> for &SourceCompPack
{
    fn eq(&self, other: &Token) -> bool 
    {
        return self.index_token == *other;
    }
}

impl Ord for SourceCompPack
{
    fn cmp(&self, other: &Self) -> Ordering 
    {
        return self.index_token.cmp(&other.index_token);
    }
}

impl PartialOrd for SourceCompPack
{
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> 
    {
        return Some(self.cmp(other));
    }
}
*/

impl SourceCompPack
{
    /// Creates new instance. The `ev_hndl` is borrowed to make sure that the
    /// provided argument originates from the source.
    /// 
    /// # Returns
    /// 
    /// * [ffi_nt_create_wait_completion_packet] - errors
    pub(crate) 
    fn new(inner: Arc<SelectorInner>, ev_hndl: BorrowedHandle<'_>, token: Token) -> io::Result<Self>
    {
        // calling NTDLL to create a competion packet
        let wcp_oh = ffi_nt_create_wait_completion_packet()?;

        // mapping our token
        let index_token = NEXT_TOKEN.next();

        return Ok(
            Self
            {
                event_hndl: 
                    ev_hndl.as_raw_handle(),
                comp_hndl: 
                    wcp_oh,
                    internal_token: index_token,
                user_token: 
                    token,
                si: 
                    inner,
                me: 
                    Weak::default(),
            }
        );
    }

    /// Sets the [Weak] reference
    fn set_ref(&mut self, me: &Weak<Mutex<SourceCompPack>>)
    {
        self.me = me.clone();
    }

    /// An event handler which is called from `event_feed` feeder.
    /// 
    /// # Panics
    /// 
    /// Because the `event_feed` in `selector` does not support soft result
    /// handling, the [Self::associate_wait_completion_packet] is `unwrapped`.
    pub(super)     
    fn from_overlapped(status: &OVERLAPPED_ENTRY, opt_events: Option<&mut Vec<Event>>) 
    {
        let cp_status = CompletionStatus::from_entry(status);

        // cast the raw pointer into the `weak` reference and try to upgrade.
        let Some(scp) = 
            unsafe 
            {
                Weak::<Mutex<SourceCompPack>>::from_raw(cp_status.overlapped() as *const Mutex<SourceCompPack>) 
            }
            .upgrade()
            else
            {
                // the owner of the object have dropped it. Ignore
                return;
            };

        // there is a potential problem with race conditions, depends on what the user will do.
        // 1) the ARC is aquired, but the mutex is locked because the reregistration is running and moved on
        //      another selector.
        // 2) user dropped `handle` before we locked mutex and won the race. So the removed event will be 
        //      returned.
        
        // lock mutex asap
        let scp_lock = 
            scp.lock().unwrap_or_else(|e| e.into_inner());

        // read event, but we cannot know if another thread is dropping the `handle`, so 
        // it will be deassociated later, but the last event (after dropping) will be emited.
        if let Some(events) = opt_events 
        {
            let ev = Event::from_completion_status_event(&cp_status); 

            events.push(ev);
        }

        // rebind

        // there is way the error can be handled. TODO: add return declaration?
        let _ = scp_lock.associate_wait_completion_packet().unwrap();

        drop(scp_lock);

        return;
    }


    /// Sets the user token
    pub(crate) 
    fn set_user_token(&mut self, token: Token)
    {
        self.user_token = token;
    }

    /// Sets the [SelectorInner].
    pub(crate) 
    fn set_selector(&mut self, si: Arc<SelectorInner>)
    {
        self.si = si;
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
    pub(super) 
    fn associate_wait_completion_packet(&self) -> io::Result<()>
    {
        unsafe
        {
            ffi::
                NtAssociateWaitCompletionPacket(
                    self.comp_hndl.as_raw_handle(), 
                    self.si.cp.as_raw_handle(), 
                    self.event_hndl, 
                    self.internal_token.get_token().0 as *mut c_void, // OVERLAPED_ENTRY.lpCompletionKey
                    Weak::into_raw(self.me.clone()).cast_mut() as *mut c_void, // OVERLAPED_ENTRY.lpOverlapped
                    0, //OVERLAPED_ENTRY.Internal
                    self.user_token.0, //OVERLAPED_ENTRY.dwNumberOfBytesTransfered
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
    pub(super)
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

static SCP_PACKS: OnceLock<Mutex<SourceCompPacks>> = OnceLock::new();

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
    scp_list: BTreeMap<usize, Arc<Mutex<SourceCompPack>>>,
}

impl SourceCompPacks
{
    /// Inserts the `scp` to the BTree.
    /// 
    /// # Returns
    /// 
    /// An [Result::Err] is returned:
    /// 
    /// * [ErrorKind::AlreadyExists] - `ev_hndl` presents on the list.
    fn insert(&mut self, ev_hndl: BorrowedHandle<'_>, scp: Arc<Mutex<SourceCompPack>>) -> io::Result<()>
    {
        let ev_handle = ev_hndl.as_raw_handle() as usize;

        if self.scp_list.contains_key(&ev_handle) == true
        {
            return Err(
                io::Error::new(ErrorKind::AlreadyExists, 
                format!("ev_handle: {} already registered!", ev_handle))
            );
        }

        let _ = self.scp_list.insert(ev_handle, scp);

        return Ok(());
    }

    /// Removes from BTree.
    /// 
    /// # Returns
    /// 
    /// An [Result::Err] is returned:
    /// 
    /// * [ErrorKind::NotFound] - `ev_hndl` was not found.
    fn remove(&mut self, ev_hndl: BorrowedHandle<'_>) -> io::Result<Arc<Mutex<SourceCompPack>>>
    {
        let loc_ev_handl = ev_hndl.as_raw_handle() as usize;

        let Some(scp) = self.scp_list.remove(&loc_ev_handl)
            else
            {
                return Err(
                    io::Error::new(ErrorKind::NotFound, 
                        format!("ev_handle: {} already registered!", loc_ev_handl))
                );
            };

        return Ok(scp);
    }
}
    
/// Adapter for [`BorrowedHandle`] providing an [`Source`] implementation.
/// 
/// `SourceHndl` enables registering only event* types with `poll`.
///
/// Works same way as `SourceFd`. The code is safe, but it lacks of the control
/// under the provided `handle`. The program which uses this methos is responsible 
/// for deregistering the [BorrowedHandle].
/// 
/// Example:
/// 
/// ```ignore
/// let mut se_hndl_timer = PrimitiveTimer::new("timer_1");
/// 
/// let mut poll = Poll::new().unwrap();
/// 
/// poll.registry().register(&mut SourceHndl::new(&se_hndl_timer).unwrap(), Token(1), Interest::READABLE).unwrap();
/// ```
#[derive(Debug)]
pub struct SourceHndl<'hndl>(BorrowedHandle<'hndl>);

impl<'hndl> Source for SourceHndl<'hndl>
{
    /// # Returns
    /// 
    /// An [Result::Err] is returned:
    /// 
    /// * [SourceCompPack::associate_wait_completion_packet] - errors
    /// 
    /// * [SourceCompPacks::insert] - errors
    fn register(&mut self, registry: &Registry, token: Token, _interests: Interest) -> io::Result<()> 
    {
        let scp_pack = SCP_PACKS.get_or_init(|| Mutex::new(SourceCompPacks::default()));

        let mut scp_lock = scp_pack.lock().unwrap_or_else(|e| e.into_inner());

        let mut scp = 
            SourceCompPack::new(registry.selector().inner.clone(), self.0, token)?;

        // creating pair event:completion
        let scp = 
            Arc::new_cyclic(|me| 
                {
                    scp.set_ref(me);

                    return Mutex::new(scp);
                }
            );

        scp_lock.insert(self.0, scp.clone())?;

        // associate
        {
            scp.lock().unwrap().associate_wait_completion_packet()?;
        }

        return Ok(());
    }

    /// # Returns
    /// 
    /// An [Result::Err] is returned:
    /// 
    /// * [SourceCompPacks::remove] - errors
    /// 
    /// * [SourceCompPack::associate_wait_completion_packet] - errors
    /// 
    /// * [SourceCompPack::deassociate_wait_completion_packet] - errors
    /// 
    /// * [SourceCompPacks::insert] - errors
    fn reregister(&mut self, registry: &Registry, token: Token, _interests: Interest) -> io::Result<()> 
    {
        let scp_pack = SCP_PACKS.get_or_init(|| Mutex::new(SourceCompPacks::default()));

        let mut scp_lock = scp_pack.lock().unwrap_or_else(|e| e.into_inner());

        let scp = scp_lock.remove(self.0)?;
        
        {
            let mut scp_lock = 
                scp.lock().unwrap_or_else(|e| e.into_inner());
            
            if registry.selector().same_port(&scp_lock.si.cp) == false
            {
                scp_lock.deassociate_wait_completion_packet(false)?;

                // update token just in case
                scp_lock.set_user_token(token);

                // replace selector
                scp_lock.set_selector(registry.selector().inner.clone());

                // register completion pack
                scp_lock.associate_wait_completion_packet()?;
            }
            else if scp_lock.user_token != token
            {
                scp_lock.deassociate_wait_completion_packet(false)?;
                
                // update token just in case
                scp_lock.set_user_token(token);

                // register completion pack
                scp_lock.associate_wait_completion_packet()?;
            }
        }
        
        // restore 
        let _ = scp_lock.insert(self.0, scp)?;
       
        return Ok(());
    }

    /// # Returns
    /// 
    /// An [Result::Err] is returned:
    /// 
    /// * [SourceCompPacks::remove] - errors
    /// 
    /// * [SourceCompPack::deassociate_wait_completion_packet] - errors
    fn deregister(&mut self, _registry: &Registry) -> io::Result<()> 
    {
        let scp_pack = SCP_PACKS.get_or_init(|| Mutex::new(SourceCompPacks::default()));

        let mut scp_lock = scp_pack.lock().unwrap_or_else(|e| e.into_inner());

        let scp = scp_lock.remove(self.0)?;
        
        scp.lock().unwrap().deassociate_wait_completion_packet(true)?;

        return Ok(());
    }
}

impl<'hndl> TryFrom<BorrowedHandle<'hndl>> for SourceHndl<'hndl>
{
    type Error = io::Error;

    fn try_from(hndl: BorrowedHandle<'hndl>) -> Result<Self, Self::Error> 
    {
        // try to verify that this is an event
        if unsafe { GetFileType(hndl.as_handle().as_raw_handle()) } != FILE_TYPE_UNKNOWN
        {
            return Err(io::Error::new(ErrorKind::InvalidData, "handler is not event type"));
        }

        return Ok(Self(hndl));
    }
}

impl<'hndl> SourceHndl<'hndl>
{
    /// Creates new instance by calling [TryFrom] from [BorrowedHandle].
    /// 
    /// The `hndl` is a `handle` which originates from 
    /// 
    /// * CreateWaitableTimer
    /// 
    /// * CreateEvent
    /// 
    /// * or other where `handle` is an event object.
    /// 
    /// # Returns
    /// 
    /// An [Result::Err] is returned:
    /// 
    /// * [ErrorKind::InvalidData] - is returned if the [GetFileType] returns other
    ///     than the type [FILE_TYPE_UNKNOWN].
    /// 
    /// Otherwise, the instance is returned.
    #[inline]
    pub 
    fn new<HNDL: AsHandle>(hndl: &'hndl HNDL) -> io::Result<Self>
    {
        return Self::try_from(hndl.as_handle());
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

    /// An atomic reference to `scp`. Should not be clonned. A [Weak] reference
    /// is passed to `selector` to detect when event `self` was dropped.
    scp_bind: Option<Arc<Mutex<SourceCompPack>>>,
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
                scp_bind: None,
            }
        );
    }

    /// Returns the [Token] as inner value if instance was registered.
    pub 
    fn get_token(&self) -> Option<Token>
    {
        return 
            self
                .scp_bind
                .as_ref()
                .map(|scp| 
                    scp.lock().unwrap_or_else(|e| e.into_inner()).user_token
                );
    }

    /// Internal function. Unregisteres the instance from the `completion_packs` list
    /// of the [SelectorInner] and removes the association.
    /// 
    /// Returns error produced by [SelectorInner::unregester_completion_pack].
    fn unreg(&mut self) -> io::Result<()>
    {
        return 
            if let Some(si) = self.scp_bind.take()
            {
                si
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .deassociate_wait_completion_packet(true)
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
    /// * [ErrorKind::AlreadyExists] - if already registered, error description [String]
    /// 
    /// * [SourceCompPack::new] - errors
    /// 
    /// * [SourceCompPack::associate_wait_completion_packet] - error
    fn register(&mut self, registry: &Registry, token: Token, _interests: Interest) -> io::Result<()> 
    {
        let ev_source = self.ev_source.get().unwrap();

        // check if already binded
        if let Some(_scp) = self.scp_bind.as_ref()
        {
            return Err(
                io::Error::new(
                    ErrorKind::AlreadyExists, 
                    format!("hndl: {} already registered", 
                        ev_source.as_handle().as_raw_handle() as usize)
                )
            )
        }

        let mut scp = 
            SourceCompPack::new(registry.selector().inner.clone(), ev_source.as_handle(), token)?;

        // creating pair event:completion
        let scp = 
            Arc::new_cyclic(|me| 
                {
                    scp.set_ref(me);

                    return Mutex::new(scp);
                }
            );

        // associate
        {
            scp.lock().unwrap().associate_wait_completion_packet()?;
        }

        // bind
        self
            .scp_bind
            .replace(scp);

        return Ok(());
    }

    /// # Returns
    /// 
    /// Errors: 
    /// 
    /// * [ErrorKind::NotFound] - if instance was not regestered previously, 
    ///     error description [String]
    /// 
    /// * [SourceCompPack::associate_wait_completion_packet] errors
    /// 
    /// * [SourceCompPack::deassociate_wait_completion_packet] errors
    fn reregister(
        &mut self,
        registry: &Registry,
        token: Token,
        _interests: Interest,
    ) -> io::Result<()> 
    {
        let ev_source = self.ev_source.get().unwrap();

        if let Some(scp) = self.scp_bind.take()
        {
            {
                let mut scp_lock = scp.lock().unwrap_or_else(|e| e.into_inner());
                
                if registry.selector().same_port(&scp_lock.si.cp) == false
                {
                    scp_lock.deassociate_wait_completion_packet(false)?;

                    // update token just in case
                    scp_lock.set_user_token(token);

                    // replace selector
                    scp_lock.set_selector(registry.selector().inner.clone());

                    // regester completion pack
                    scp_lock.associate_wait_completion_packet()?;
                }
                else if scp_lock.user_token != token
                {
                    scp_lock.deassociate_wait_completion_packet(false)?;
                    
                    // update token just in case
                    scp_lock.set_user_token(token);

                    // regester completion pack
                    scp_lock.associate_wait_completion_packet()?;
                }
            }
            
            // restore 
            let _ = self.scp_bind.replace(scp);

            // otherwise nothing has changed

            return Ok(());
        }
        else
        {
             return Err(
                io::Error::new(
                    ErrorKind::NotFound, 
                    format!("hndl: {} not registred with Registry", 
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
    /// * [SourceCompPack::deassociate_wait_completion_packet] - errors
    fn deregister(&mut self, registry: &Registry) -> io::Result<()> 
    {
        if let Some(scp) = self.scp_bind.take()
        {
            {
                let scp_lock = scp.lock().unwrap_or_else(|e| e.into_inner());

                if registry.selector().same_port(&scp_lock.si.cp) == false
                {
                    // restore
                    self.scp_bind.replace(scp.clone());

                    return Err(
                        io::Error::new(
                            ErrorKind::CrossesDevices, 
                            format!("hndl: {} token: {} is on different port", 
                                self.ev_source.get().unwrap().as_handle().as_raw_handle() as usize,
                                scp_lock.user_token.0)
                        )
                    )
                }

                // deregister the instance
                scp_lock.deassociate_wait_completion_packet(true)?;
            }

            drop(scp);
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


