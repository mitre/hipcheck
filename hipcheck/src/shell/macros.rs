//! Macros that mirror/replace those from the standard library using the global [`Shell`][crate::shell::Shell].

macro_rules! println {
    ($($arg:tt)+) => {
        $crate::shell::Shell::println(format!($($arg)*));
    };

    () => {
        $crate::shell::Shell::println("");
    }
}

// public re-export
pub(crate) use println;

macro_rules! eprintln {
    ($($arg:tt)+) => {
        $crate::shell::Shell::eprintln(format!($($arg)*));
    };

    () => {
        $crate::shell::Shell::eprintln("");
    }
}

pub(crate) use eprintln;
