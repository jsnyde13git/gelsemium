fn main() {
    // I think it's reasonable to crash the build on a Slint compile error.
    #[allow(clippy::expect_used)]
    slint_build::compile("ui/app-window.slint").expect("Slint build failed");
}
