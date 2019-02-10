#![no_std]
#![feature(generator_trait, generators, futures_api, async_await, await_macro)]

use cofutures_macro::coasync;

#[coasync]
fn test() -> i32 {
	yield;
	42
}

#[coasync]
fn foo() -> i32 {
	await!(test())
}

mod sleep {
	extern crate std;
	use std::sync::{Arc, Mutex};

	pub struct Sleep(Arc<Mutex<(bool, Option<core::task::Waker>)>>);

	impl Sleep {
		pub fn new() -> Self {
			Sleep(Arc::new(Mutex::new((false, None))))
		}
	}

	impl core::future::Future for Sleep {
		type Output = ();

		fn poll(self: core::pin::Pin<&mut Self>, lw: &core::task::LocalWaker) -> core::task::Poll<Self::Output> {
			let mut inner_w = self.0.lock().unwrap();
			let first = inner_w.1.is_none();
			inner_w.1 = Some(lw.clone().into_waker());
			if first {
				let handle = self.0.clone();
				// we need to register with something that wakes the
				// task up. tokio often uses global/TLS contexts to find
				// those (e.g. IO polling is run in a separate thread by
				// default afaik), but it doesn't have to be this way.
				//
				// for a single-thread executor you need to combine a
				// main executor engine with IO/timer/... contexts, e.g.
				// like:
				// https://github.com/tokio-rs/tokio/blob/9a8d087c/src/runtime/current_thread/runtime.rs#L27
				std::thread::spawn(move || {
					std::thread::sleep(std::time::Duration::from_millis(1000));
					let mut inner = handle.lock().unwrap();
					inner.0 = true;
					inner.1.as_ref().unwrap().wake();
				});
				core::task::Poll::Pending
			} else if inner_w.0 {
				core::task::Poll::Ready(())
			} else {
				core::task::Poll::Pending
			}
		}
	}
}

fn main() {
	extern crate std;
	use std::println;

	println!("{}", futures_executor::block_on(foo()));
	println!("{:?}", futures_executor::block_on(sleep::Sleep::new()));
	println!("slept");
}
