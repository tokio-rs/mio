use mio::Interest;

#[test]
fn is_tests() {
    assert!(Interest::READABLE.is_readable());
    assert!(!Interest::READABLE.is_writable());
    assert!(!Interest::WRITABLE.is_readable());
    assert!(Interest::WRITABLE.is_writable());
    assert!(!Interest::WRITABLE.is_aio());
    assert!(!Interest::WRITABLE.is_lio());
}

#[test]
fn bit_or() {
    let interests = Interest::READABLE | Interest::WRITABLE;
    assert!(interests.is_readable());
    assert!(interests.is_writable());
}

#[test]
fn fmt_debug() {
    assert_eq!(format!("{:?}", Interest::READABLE), "READABLE");
    assert_eq!(format!("{:?}", Interest::WRITABLE), "WRITABLE");
    assert_eq!(
        format!("{:?}", Interest::READABLE | Interest::WRITABLE),
        "READABLE | WRITABLE"
    );
    #[cfg(any(
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "ios",
        target_os = "macos",
    ))]
    {
        assert_eq!(format!("{:?}", Interest::AIO), "AIO");
    }
    #[cfg(any(target_os = "freebsd"))]
    {
        assert_eq!(format!("{:?}", Interest::LIO), "LIO");
    }
}

#[test]
fn add() {
    let interest: Interest = Interest::READABLE.add(Interest::WRITABLE);

    assert!(interest.is_readable());
    assert!(interest.is_writable());
}
