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

fn main() {
	// println!() needs std :)
	let _ = futures_executor::block_on(foo());
}
