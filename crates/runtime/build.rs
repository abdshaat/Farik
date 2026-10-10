//! Builds `catervas-runtime` again when the web app is built again. The embed of `apps/web/dist` is
//! fixed when the crate is compiled: a release build holds its files, and a debug build, which reads
//! them at run time, still answers "not built" when the folder was missing at compile time. So both
//! watch the folder, or, while it is missing, `apps/web`, whose listing changes when it appears.

fn main() {
    let watched = if std::path::Path::new("../../apps/web/dist").exists() {
        "../../apps/web/dist"
    } else {
        "../../apps/web"
    };
    println!("cargo:rerun-if-changed={watched}");
}
