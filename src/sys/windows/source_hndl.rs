use std::
{
    cell::OnceCell, collections::BTreeMap, io::{self, ErrorKind}, os::
    {
        raw::c_void, 
        windows::io::{AsHandle, AsRawHandle, BorrowedHandle, OwnedHandle, RawHandle}
    }, ptr::null_mut, sync::{Arc, Mutex, OnceLock}, usize
};

use windows_sys::Win32::{Storage::FileSystem::{FILE_TYPE_UNKNOWN, GetFileType}, System::IO::{OVERLAPPED, OVERLAPPED_ENTRY}};

use crate::
{
    Interest, 
    Registry, 
    Token, 
    event::Source, 
    sys::windows::{Event, ffi::{self, IO_CANCELLED_ERROR, ffi_nt_create_wait_completion_packet}, iocp::{CompletionPort, CompletionStatus}, overlapped::Overlapped, tokens::{TokenEvent, TokenGenerator, TokenSelector}}
};

use super::ffi::HANDLE;


static NEXT_TOKEN: TokenGenerator<TokenEvent> = TokenGenerator::new();

/// A type alias for callback
type OcCallbackType = fn(&OverlappedCallback, &OVERLAPPED_ENTRY, Option<&mut Vec<Event>>);

/// Sizeof OVERLAPPED is 32 
#[repr(C)]
#[derive(Debug, Clone)]
pub(crate) struct OverlappedCallback
{
    pad: [u8; size_of::<OVERLAPPED>()],

    /// A reference to self. Keeps the instance until table is destructed.
    me: Arc<Mutex<SourceCompPack>>,

    /// A callback
    callback_ptr: OcCallbackType,
}

unsafe impl Send for OverlappedCallback {}
unsafe impl Sync for OverlappedCallback {}

impl From<*mut OVERLAPPED> for OverlappedCallback
{
    fn from(value: *mut OVERLAPPED) -> OverlappedCallback
    {
        let olc = value as *mut Self;

        let overlp_callback = unsafe {olc.as_ref().unwrap() };

        return overlp_callback.clone();
    }
}

impl OverlappedCallback
{
    fn new(callback_ptr: OcCallbackType, me: Arc<Mutex<SourceCompPack>>) -> Self
    {
        return Self{ pad: [0_u8; size_of::<OVERLAPPED>()], callback_ptr: callback_ptr, me: me };
    }

    pub(crate) 
    fn call(self, ov_ent: &OVERLAPPED_ENTRY, event_opt: Option<&mut Vec<Event>>)
    {
        (self.callback_ptr)(&self, ov_ent, event_opt);
    }
}

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
/// `user_token` is assigned by user and visible only to user.
/// 
/// `callbacks` is a table of callbacks. The callback with ID=1
/// [SourceCompPack::CALL_BACK_DROP] can be posted to port in 
/// order to perform instance destruction.
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
    cp: Arc<CompletionPort>,

    /// Callback table: 0 - event, 1 - drop
    callbacks: OnceCell<[OverlappedCallback; 2]>,
}

unsafe impl Sync for SourceCompPack {}

unsafe impl Send for SourceCompPack {}

#[cfg(test)]
impl Drop for SourceCompPack
{
    fn drop(&mut self) 
    {
        println!("debug: ~SourceCompPack");
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

impl SourceCompPack
{
    const CALL_BACK_DROP: isize = 1;

    /// Creates new instance. The `ev_hndl` is borrowed to make sure that the
    /// provided argument originates from the source.
    /// 
    /// # Returns
    /// 
    /// * [ffi_nt_create_wait_completion_packet] - errors
    pub(crate) 
    fn new(comp_pack: Arc<CompletionPort>, ev_hndl: BorrowedHandle<'_>, token: Token) -> io::Result<Self>
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
                cp: 
                    comp_pack,
                callbacks:
                    OnceCell::new(),
            }
        );
    }

    fn setup_callback(&mut self, scp: &Arc<Mutex<SourceCompPack>>)
    {
        let callbacks = 
            [
                OverlappedCallback::new(Self::callback_ev_event, scp.clone()),
                OverlappedCallback::new(Self::callback_ev_drop, scp.clone()),
            ];

        self.callbacks.set(callbacks).unwrap();
    }

    pub(super) 
    fn callback_ev_event(this: &OverlappedCallback, _status: &OVERLAPPED_ENTRY, opt_events: Option<&mut Vec<Event>>) 
    {
        // lock mutex asap
        let scp_lock = 
            this.me.lock().unwrap_or_else(|e| e.into_inner());

        // read event, but we cannot know if another thread is dropping the `handle`, so 
        // it will be deassociated later, but the last event (after dropping) will be emited.
        if let Some(events) = opt_events 
        {
            events.push(Event::new(scp_lock.user_token));
        }

        // rebind
        // there is way the error can be handled. TODO: add return declaration?
        let _ = scp_lock.associate_wait_completion_packet().unwrap();

        drop(scp_lock);

        return;
    }

    /// Handling a request to drop the instance.
    pub(super) 
    fn callback_ev_drop(this: &OverlappedCallback, _status: &OVERLAPPED_ENTRY, _opt_events: Option<&mut Vec<Event>>) 
    {
        // lock mutex asap
        let mut scp_lock = 
            this.me.lock().unwrap_or_else(|e| e.into_inner());

        // clear all callbacks (2 strong counts including 'this', the base ref should already be gone)
        // so at that moment 2 strong refs should be left
        let _ = scp_lock.callbacks.take();

        drop(scp_lock);

        assert_eq!(Arc::strong_count(&this.me), 1);

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
    fn set_comp_pack(&mut self, cp: Arc<CompletionPort>)
    {
        self.cp = cp;
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
    { //Weak::into_raw(self.me.clone()).cast_mut() as *mut c_void
        unsafe
        {
            ffi::
                NtAssociateWaitCompletionPacket(
                    self.comp_hndl.as_raw_handle(), 
                    self.cp.as_raw_handle(), 
                    self.event_hndl, 
                    self.internal_token.get_token().0 as *mut c_void, // OVERLAPED_ENTRY.lpCompletionKey
                    self.callbacks.get().unwrap().as_ptr() as *mut c_void, // OVERLAPED_ENTRY.lpOverlapped
                    0, //OVERLAPED_ENTRY.Internal
                    0, //OVERLAPED_ENTRY.dwNumberOfBytesTransfered
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

    /// Sends to the port the callback to deregister the instance.
    pub(super) 
    fn post_event_drop(&self) -> io::Result<()>
    {
        let is_msg = 
            CompletionStatus::new(
                0, 
                self.internal_token, 
                unsafe 
                { 
                    self.callbacks.get().unwrap().as_ptr().offset(Self::CALL_BACK_DROP) as *mut Overlapped 
                }
            );
        
        return self.cp.post(is_msg);
    }
}

static SCP_PACKS: OnceLock<Mutex<SourceCompPacks>> = OnceLock::new();

/// A wait completion packet storage. (Is needed for [SourceHndl]).
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

        let mut scp_pack_lock = scp_pack.lock().unwrap_or_else(|e| e.into_inner());

        let scp = Arc::new( Mutex::new(
            SourceCompPack::new(registry.selector().clone_port(), self.0, token)?
        ));

        // setting callback table
        let mut scp_lock = scp.lock().unwrap();

        scp_lock.setup_callback(&scp);

        scp_pack_lock.insert(self.0, scp.clone())?;

        drop(scp_pack_lock);

        // associate
        scp_lock.associate_wait_completion_packet()?;

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
            
            if registry.selector().same_port(&scp_lock.cp) == false
            {
                scp_lock.deassociate_wait_completion_packet(true)?;

                // update token just in case
                scp_lock.set_user_token(token);

                // replace selector
                scp_lock.set_comp_pack(registry.selector().clone_port());

                // register completion pack
                scp_lock.associate_wait_completion_packet()?;
            }
            else if scp_lock.user_token != token
            {
                scp_lock.deassociate_wait_completion_packet(true)?;
                
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

        let mut scp_pack_lock = scp_pack.lock().unwrap_or_else(|e| e.into_inner());

        let scp = scp_pack_lock.remove(self.0)?;
        
        let scp_lock = 
            scp
                .lock()
                .unwrap_or_else(|e| e.into_inner());
        
        scp_lock.deassociate_wait_completion_packet(true)?;
        scp_lock.post_event_drop()?;
        

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

    /// An atomic reference to `scp`. Must never be clonned. The instance should
    /// drop the reference firstly before calling the drop for [SourceCompPack].
    scp_bind: Option<Arc<Mutex<SourceCompPack>>>,
}

unsafe impl<HNDL: AsHandle> Send for SourceEventHndl<HNDL> {}
unsafe impl<HNDL: AsHandle> Sync for SourceEventHndl<HNDL> {}

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

    /// 1) deassociates the wait completion packet
    /// 
    /// 2) posts the event drop
    /// 
    /// Shold be used for deregistration purposes only! Locks mutex.
    fn unreg(&mut self) -> io::Result<()>
    {
        return 
            if let Some(scp) = self.scp_bind.take()
            {
                let scp_lock = scp.lock().unwrap_or_else(|e| e.into_inner());
                
                scp_lock
                    .deassociate_wait_completion_packet(true)
                    .map(|_| ())?;

                scp_lock.post_event_drop()
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

        let scp = Arc::new( Mutex::new(
            SourceCompPack::new(registry.selector().clone_port(), ev_source.as_handle(), token)?
        ));
        
        // setting callback table
        let mut scp_lock = scp.lock().unwrap();

        scp_lock.setup_callback(&scp);

        scp_lock.associate_wait_completion_packet()?;

        drop(scp_lock);

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
                
                if registry.selector().same_port(&scp_lock.cp) == false
                {
                    scp_lock.deassociate_wait_completion_packet(true)?;

                    // update token just in case
                    scp_lock.set_user_token(token);

                    // replace selector
                    scp_lock.set_comp_pack(registry.selector().clone_port());

                    // regester completion pack
                    scp_lock.associate_wait_completion_packet()?;
                }
                else if scp_lock.user_token != token
                {
                    scp_lock.deassociate_wait_completion_packet(true)?;
                    
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

                if registry.selector().same_port(&scp_lock.cp) == false
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
                scp_lock
                    .deassociate_wait_completion_packet(true)
                    .map(|_| ())?;

                scp_lock.post_event_drop()?;
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


