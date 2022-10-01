cfg_epoll_selector! {
    pub(super) mod stateless;
}

cfg_kqueue_selector! {
    pub(super) mod stateless;
}

cfg_poll_selector! {
    pub(super) mod edge_triggered;
}
