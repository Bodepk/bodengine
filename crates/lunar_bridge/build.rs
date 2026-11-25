fn main() {
    cxx_build::bridge("src/lib.rs")
        .include("include")
        .std("c++17")
        .compile("lunar_bridge");
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=include/lunar_bridge/game.h");
}
