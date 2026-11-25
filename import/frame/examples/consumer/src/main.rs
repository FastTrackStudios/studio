//! A surface played against the mock engine, through Frame's public API:
//! the core's markup and runtime, frame-architect's engine.

use frame::surface::runtime::sync::{self, Local};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let engine = frame_architect::memory::Memory::demo();
    let scene = frame::document::proto::Scene::new("Panel");
    let surface = frame::surface::runtime::Surface::from_scene(scene);
    let rt = pollster_free_block_on(sync::connect(&Local(&engine), surface))?;
    println!("{} params", rt.catalog().params.len());
    Ok(())
}

/// Drive a future that never waits (the in-process engine answers at once).
fn pollster_free_block_on<F: std::future::Future>(f: F) -> F::Output {
    let waker = std::task::Waker::noop();
    let mut cx = std::task::Context::from_waker(waker);
    let mut f = std::pin::pin!(f);
    loop {
        if let std::task::Poll::Ready(out) = f.as_mut().poll(&mut cx) {
            return out;
        }
    }
}
