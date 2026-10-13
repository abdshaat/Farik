//! Two jobs for `catervas-runtime`'s build.
//!
//! It builds the crate again when the web app is built again. The embed of `apps/web/dist` is
//! fixed when the crate is compiled: a release build holds its files, and a debug build, which reads
//! them at run time, still answers "not built" when the folder was missing at compile time. So both
//! watch the folder, or, while it is missing, `apps/web`, whose listing changes when it appears.
//!
//! It also hands over `CATERVAS_SANDBOX_DIGEST`, the first 6 bytes of the sha256 of
//! `sandbox/Dockerfile` as 12 lowercase hex digits, which `SANDBOX_IMAGE`'s tag carries, so a changed
//! Dockerfile is a new image tag (`docs/SPEC.md` 8.3).

use sha2::{Digest as _, Sha256};

fn main() {
    let watched = if std::path::Path::new("../../apps/web/dist").exists() {
        "../../apps/web/dist"
    } else {
        "../../apps/web"
    };
    println!("cargo:rerun-if-changed={watched}");

    let dockerfile = std::fs::read("sandbox/Dockerfile")
        .unwrap_or_else(|error| panic!("cannot read sandbox/Dockerfile: {error}"));
    let digest = Sha256::digest(&dockerfile);
    let hex: String = digest[..6]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    println!("cargo:rustc-env=CATERVAS_SANDBOX_DIGEST={hex}");
    println!("cargo:rerun-if-changed=sandbox/Dockerfile");
}
