//! studio-web's program, as `dx serve` and `dx bundle` build it: the
//! page's start is the library's (`studio_web::start`).

fn main() {
    #[cfg(target_arch = "wasm32")]
    studio_web::start();
}
