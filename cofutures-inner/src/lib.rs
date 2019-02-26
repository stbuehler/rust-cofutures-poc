#![no_std]
#![feature(generator_trait, futures_api)]

use core::ops::Generator;
use core::ops::GeneratorState;
use core::pin::Pin;
use core::task::Waker;
use core::task::Poll;
use core::future::Future;

#[doc(hidden)]
// used for a local variable in the generator which can be used to
// access the current `&Waker` passed to `Future::poll`.
pub struct WakerContext(*const *const Waker);
unsafe impl Send for WakerContext {}

impl WakerContext {
	pub unsafe fn with<F, R>(&self, f: F) -> R
	where
		F: FnOnce(&Waker) -> R,
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

// struct implenting Future for wrapped generators
#[doc(hidden)]
pub struct CoAsync<Output, T, F>
where
	T: Generator<Yield = (), Return = Output>,
	F: FnOnce(WakerContext) -> T,
{
	// state needs to be Option so we can temporarily take it.
	// might end up empty if generator init panics.
	state: Option<CoAsyncState<Output, T, F>>,
	last_waker: *const Waker,
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

	fn poll(self: Pin<&mut Self>, lw: &Waker) -> Poll<Self::Output> {
		let this = unsafe { Pin::get_unchecked_mut(self) }; // -> get_mut_unchecked (got renamed in some nightly version)
		this.last_waker = lw;
		if let Some(CoAsyncState::Init(_)) = this.state {
			// now that we're pinned we can pass the (now stable)
			// pointer to `&this.last_waker` to the generator, so only
			// now we actually create the generator object.
			match this.state.take() {
				Some(CoAsyncState::Init(init)) => {
					this.state = Some(CoAsyncState::Running(init(WakerContext(&this.last_waker))));
				}
				_ => unreachable!(),
			}
		}
		match &mut this.state {
			Some(CoAsyncState::Running(ref mut running)) => {
				match unsafe { Pin::new_unchecked(running).resume() } {
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
