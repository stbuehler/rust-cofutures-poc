#![no_std]
#![feature(generator_trait, futures_api)]

use core::ops::Generator;
use core::ops::GeneratorState;
use core::pin::Pin;
use core::task::LocalWaker;
use core::task::Poll;
use core::future::Future;

#[doc(hidden)]
pub struct WakerContext(*const *const LocalWaker);
unsafe impl Send for WakerContext {}

impl WakerContext {
	pub unsafe fn with<F, R>(&self, f: F) -> R
	where
		F: FnOnce(&LocalWaker) -> R,
	{
		let waker = &**self.0;
		f(waker)
	}

	pub unsafe fn poll<F, R>(&self, f: Pin<&mut F>) -> Poll<R>
	where
		F: Future<Output = R>,
	{
		self.with(|waker| {
			f.poll(waker)
		})
	}

	pub unsafe fn wake(&self) {
		self.with(|waker| waker.wake());
	}
}

enum CoAsyncState<Output, T, F>
where
	T: Generator<Yield = (), Return = Output>,
	F: FnOnce(WakerContext) -> T,
{
	Init(F),
	Running(T),
}

pub struct CoAsync<Output, T, F>
where
	T: Generator<Yield = (), Return = Output>,
	F: FnOnce(WakerContext) -> T,
{
	state: Option<CoAsyncState<Output, T, F>>,
	last_waker: *const LocalWaker,
}

impl<Output, T, F> CoAsync<Output, T, F>
where
	T: Generator<Yield = (), Return = Output>,
	F: FnOnce(WakerContext) -> T,
{
	pub unsafe fn new(init: F) -> Self {
		CoAsync {
			state: Some(CoAsyncState::Init(init)),
			last_waker: core::ptr::null(),
		}
	}
}

impl<Output, T, F> Future for CoAsync<Output, T, F>
where
	T: Generator<Yield = (), Return = Output>,
	F: FnOnce(WakerContext) -> T,
{
	type Output = Output;

	fn poll(self: Pin<&mut Self>, lw: &LocalWaker) -> Poll<Self::Output> {
		let this = unsafe { Pin::get_unchecked_mut(self) }; // -> get_mut_unchecked ?
		this.last_waker = lw;
		if let Some(CoAsyncState::Init(_)) = this.state {
			match this.state.take() {
				Some(CoAsyncState::Init(init)) => {
					this.state = Some(CoAsyncState::Running(init(WakerContext(&this.last_waker))));
				}
				_ => unreachable!(),
			}
		}
		match &mut this.state {
			Some(CoAsyncState::Running(ref mut running)) => {
				match unsafe { running.resume() } {
					GeneratorState::Complete(y) => {
						Poll::Ready(y)
					}
					GeneratorState::Yielded(()) => {
						Poll::Pending
					}
				}
			},
			_ => unreachable!(),
		}
	}
}
