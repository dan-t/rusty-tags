#[macro_use]

macro_rules! info {
    ($config:ident, $fmt:expr) => {{
        if ! $config.quiet {
            println!($fmt);
        }
    }};

    ($config:ident, $fmt:expr, $($arg:tt)*) => {{
        if ! $config.quiet {
            println!($fmt, $($arg)*);
        }
    }};
}

macro_rules! verbose {
    ($config:ident, $fmt:expr) => {{
        if $config.verbose {
            println!($fmt);
        }
    }};

    ($config:ident, $fmt:expr, $($arg:tt)*) => {{
        if $config.verbose {
            println!($fmt, $($arg)*);
        }
    }};
}

macro_rules! verbose_ {
    ($fmt:expr) => {{
        println!($fmt);
    }};

    ($fmt:expr, $($arg:tt)*) => {{
        println!($fmt, $($arg)*);
    }};
}
