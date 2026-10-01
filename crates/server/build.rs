//! Turns on `cfg(embed_assets)` when `KLOTHO_ASSETS_DIR` points at a Topcoat asset
//! bundle, as `cargo xtask dist` does for its second build pass (ADR 0002).
//!
//! A cfg emitted here applies to this crate only, so the second pass compiles
//! every other crate, klotho-web included, exactly as the first one did, and the
//! asset IDs in the bundle stay valid.

fn main() {
    println!("cargo::rustc-check-cfg=cfg(embed_assets)");
    println!("cargo::rerun-if-env-changed=KLOTHO_ASSETS_DIR");
    if std::env::var_os("KLOTHO_ASSETS_DIR").is_some_and(|dir| !dir.is_empty()) {
        println!("cargo::rustc-cfg=embed_assets");
    }
}
