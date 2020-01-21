cfg_epoll! {
    mod epoll;
    pub(crate) use self::epoll::{event, Event, Events, Selector};
}

cfg_kqueue! {
    mod kqueue;
    pub(crate) use self::kqueue::{event, Event, Events, Selector};
}

cfg_neither_epoll_nor_kqueue! {
    mod poll;
    pub(crate) use self::poll::{event, Event, Events, Selector};
}
