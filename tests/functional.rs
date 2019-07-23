#[path = "./util/mod.rs"]
mod util;

// This macro is required to get around rustfmt bugs #3572 and/or #1208, also
// see Mio pr #1030.
macro_rules! test_mod {
    ($( $name: ident ),*) => {
        mod functional {
            $( test_mod!(_ $name, stringify!($name)); )*
        }
    };
    (_ $name: ident, $pname: expr) => {
        test_mod!(__ concat!("./functional/", $pname, ".rs"), mod $name;);
    };
    (__ $path: expr, $($tt:tt)*) => {
        #[path = $path]
        $($tt)*
    };
}

test_mod!(event_source, interests, poll, selector_id, tcp, udp, waker);
