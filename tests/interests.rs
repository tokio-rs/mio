use mio::Interests;

#[test]
fn is_tests() {
    assert!(Interests::READABLE.is_readable());
    assert!(!Interests::READABLE.is_writable());
    assert!(!Interests::WRITABLE.is_readable());
    assert!(Interests::WRITABLE.is_writable());
    assert!(!Interests::WRITABLE.is_aio());
    assert!(!Interests::WRITABLE.is_lio());
}

#[test]
fn bit_or() {
    let interests = Interests::READABLE | Interests::WRITABLE;
    assert!(interests.is_readable());
    assert!(interests.is_writable());
}

#[test]
fn fmt_debug() {
    assert_eq!(format!("{:?}", Interests::READABLE), "READABLE");
    assert_eq!(format!("{:?}", Interests::WRITABLE), "WRITABLE");
    assert_eq!(
        format!("{:?}", Interests::READABLE | Interests::WRITABLE),
        "READABLE | WRITABLE"
    );
    #[cfg(any(
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "ios",
        target_os = "macos"
    ))]
    {
        assert_eq!(format!("{:?}", Interests::AIO), "AIO");
    }
    #[cfg(any(target_os = "freebsd"))]
    {
        assert_eq!(format!("{:?}", Interests::LIO), "LIO");
    }
}
