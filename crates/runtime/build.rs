//! Builds `farik-runtime` again when the web app is built again. A release build embeds
//! `apps/web/dist`, and its macro cannot tell cargo that a new file came into the folder; a debug
//! build reads the folder at run time, so it needs no rebuild, and cargo, which counts a missing
//! path as changed, would otherwise rebuild the crate every time while the app is not built.

fn main() {
    if std::env::var("PROFILE").as_deref() == Ok("release") {
        println!("cargo:rerun-if-changed=../../apps/web/dist");
    } else {
        println!("cargo:rerun-if-changed=build.rs");
    }
}
