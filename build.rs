fn main() {
    let path = "./build";
    let lib = "sqip";

    println!("cargo:rustc-link-search=native={}", path);
    println!("cargo:rustc-link-lib=static={}", lib);
}