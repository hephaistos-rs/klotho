fn main() {
    // Generates the Tailwind stylesheet into OUT_DIR. The standalone Tailwind CLI is
    // downloaded once and cached, so no Node.js is needed (NFR-OPS-005).
    topcoat::tailwind::BuildConfig::new().input("styles.css").render().unwrap();
}
