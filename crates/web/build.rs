use topcoat::icon::iconify;
use topcoat::tailwind::BuildConfig;

/// The Tailwind CLI release, and its SHA-256 per platform from the release's
/// `sha256sums.txt`. Bump all of them together.
const TAILWIND_VERSION: &str = "4.3.2";
const TAILWIND_SHA256: &[(&str, &str, &str)] = &[
    ("linux", "x86_64", "5036c4fb4328e0bcdbb6065c70d8ac9452e0d4c947113a788a8f94fd390425c1"),
    ("linux", "aarch64", "394ddccc2402cfa3abd97dfba56f3587781a3d6e6ce66e65ceada14beb7664b8"),
    ("macos", "x86_64", "cef8f110471e889c3c4409055cf8aff33076f58a081867b0dfc6534b290bfbb0"),
    ("macos", "aarch64", "b800b0659dc64b9f03ede5660244d9415d777d5739ae2889280877ca37be742a"),
    ("windows", "x86_64", "224a62a8351d3b8da9d950a4eb1d7176dc901dc4735b47f816f3dfcbc67d8654"),
];

/// The Lucide icon set (`@iconify-json/lucide`), vendored in `icons/` so
/// builds need no network. Bumping the version downloads the new set once.
const LUCIDE_VERSION: &str = "1.2.138";

fn main() {
    // Printing any `rerun-if` line makes Cargo watch only what is listed, so
    // everything that shapes the stylesheet and the icons is listed here.
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=styles.css");
    println!("cargo:rerun-if-changed=icons");
    println!("cargo:rerun-if-env-changed=TAILWIND_CLI");

    iconify::BuildConfig::new()
        .cache_dir("icons")
        .icon_set_version("lucide", LUCIDE_VERSION)
        .stage()
        .unwrap();

    // Generates the Tailwind stylesheet into OUT_DIR. The standalone Tailwind
    // CLI is downloaded once, checked against its checksum and cached, so no
    // Node.js is needed (NFR-OPS-005). Set `TAILWIND_CLI` to a local
    // executable to build offline or with a packaged CLI. A platform with no
    // checksum here fails the build: the CLI is never downloaded unverified.
    let config = BuildConfig::new().input("styles.css");
    let config = if std::env::var_os("TAILWIND_CLI").is_some() {
        config.executable_env("TAILWIND_CLI")
    } else {
        let host = (std::env::consts::OS, std::env::consts::ARCH);
        let Some((_, _, sha)) = TAILWIND_SHA256.iter().find(|(os, arch, _)| (*os, *arch) == host) else {
            panic!(
                "no Tailwind CLI {TAILWIND_VERSION} checksum for {}-{}: set TAILWIND_CLI to a Tailwind CLI executable",
                host.0, host.1
            );
        };
        config.version_checksum(TAILWIND_VERSION, format!("sha256:{sha}"))
    };
    config.render().unwrap();
}
