use std::env;

fn main() {
    if env::var_os("TSUKUYOMI_DENY_WARNINGS").is_some() {
        println!("cargo:rustc-cfg=tsukuyomi_deny_warnings");
    }
}
