#![recursion_limit="256"]

extern crate proc_macro;
#[macro_use]
extern crate syn;
#[macro_use]
extern crate quote;
extern crate proc_macro2;

use proc_macro::TokenStream;
use syn::export::ToTokens;
use syn::ItemFn;

fn waker_ident() -> syn::Ident {
	// API not stable yet; hide name for now with some entropy
	// syn::Ident::new("waker", proc_macro2::Span::def_site())
	syn::Ident::new("waker_B4rT4QBtWIAFEDtLJ", proc_macro2::Span::call_site())
}

fn path_is_await(path: &syn::Path) -> bool {
	if path.leading_colon.is_none() && path.segments.len() == 1 {
		let seg = &path.segments[0];
		let await_ident = syn::Ident::new("await", seg.ident.span());
		seg.arguments == syn::PathArguments::None && seg.ident == await_ident
	} else {
		false
	}
}

fn build_await(expr: &syn::Expr) -> syn::Expr {
	let waker = waker_ident();
	syn::parse(quote!{{
		let mut pinned = #expr;
		loop {
			if let core::task::Poll::Ready(x) = unsafe { #waker.poll(
					core::pin::Pin::new_unchecked(&mut pinned)
			)}
			{
				break x;
			}
			yield
		}
	}}.into()).unwrap()
}

fn build_yield() -> syn::Expr {
	let waker = waker_ident();
	syn::parse(quote!{{
		unsafe { #waker.wake(); } // run again ASAP
		yield
	}}.into()).unwrap()
}

trait HandleAwait {
	fn handle_await(&mut self);
}

impl<T: HandleAwait> HandleAwait for Option<T> {
	fn handle_await(&mut self) {
		if let Some(h) = self {
			h.handle_await();
		}
	}
}

impl<T: HandleAwait> HandleAwait for Box<T> {
	fn handle_await(&mut self) {
		(**self).handle_await()
	}
}

impl HandleAwait for syn::ItemFn {
	fn handle_await(&mut self) {
		self.block.handle_await();
	}
}

impl HandleAwait for syn::Block {
	fn handle_await(&mut self) {
		for stmt in self.stmts.iter_mut() {
			stmt.handle_await();
		}
	}
}

impl HandleAwait for syn::Stmt {
	fn handle_await(&mut self) {
		match self {
			syn::Stmt::Local(ref mut local) => local.handle_await(),
			syn::Stmt::Item(_) => (),
			syn::Stmt::Expr(ref mut e) => e.handle_await(),
			syn::Stmt::Semi(ref mut e, _) => e.handle_await(),
		}
	}
}

impl HandleAwait for syn::Local {
	fn handle_await(&mut self) {
		if let Some((_, ref mut e)) = self.init {
			e.handle_await();
		}
	}
}

impl HandleAwait for syn::Expr {
	fn handle_await(&mut self) {
		let mut new_expr: Option<syn::Expr> = None;

		match self {
			syn::Expr::Box(e) => e.expr.handle_await(),
			syn::Expr::InPlace(e) => e.value.handle_await(),
			syn::Expr::Array(e) => {
				for i in e.elems.iter_mut() {
					i.handle_await();
				}
			},
			syn::Expr::Call(e) => {
				e.func.handle_await();
				for i in e.args.iter_mut() {
					i.handle_await();
				}
			},
			syn::Expr::MethodCall(e) => {
				e.receiver.handle_await();
				for i in e.args.iter_mut() {
					i.handle_await();
				}
			},
			syn::Expr::Tuple(e) => {
				for i in e.elems.iter_mut() {
					i.handle_await();
				}
			},
			syn::Expr::Binary(e) => {
				e.left.handle_await();
				e.right.handle_await();
			},
			syn::Expr::Unary(e) => e.expr.handle_await(),
			syn::Expr::Lit(_) => (),
			syn::Expr::Cast(e) => e.expr.handle_await(),
			syn::Expr::Type(e) => e.expr.handle_await(),
			syn::Expr::Let(e) => e.expr.handle_await(),
			syn::Expr::If(e) => {
				e.cond.handle_await();
				e.then_branch.handle_await();
				if let Some((_, ref mut else_branch)) = e.else_branch {
					else_branch.handle_await();
				};
			},
			syn::Expr::While(e) => {
				e.cond.handle_await();
				e.body.handle_await();
			},
			syn::Expr::ForLoop(e) => {
				e.expr.handle_await();
				e.body.handle_await();
			},
			syn::Expr::Loop(e) => {
				e.body.handle_await();
			},
			syn::Expr::Match(e) => {
				e.expr.handle_await();
				for arm in e.arms.iter_mut() {
					if let Some((_, ref mut guard)) = arm.guard {
						guard.handle_await();
					}
					arm.body.handle_await();
				}
			},
			syn::Expr::Closure(_) => (),
			syn::Expr::Unsafe(e) => e.block.handle_await(),
			syn::Expr::Block(e) => e.block.handle_await(),
			syn::Expr::Assign(e) => {
				e.left.handle_await();
				e.right.handle_await();
			},
			syn::Expr::AssignOp(e) => {
				e.left.handle_await();
				e.right.handle_await();
			},
			syn::Expr::Field(e) => e.base.handle_await(),
			syn::Expr::Index(e) => {
				e.expr.handle_await();
				e.index.handle_await();
			},
			syn::Expr::Range(e) => {
				e.from.handle_await();
				e.to.handle_await();
			},
			syn::Expr::Path(_) => (),
			syn::Expr::Reference(e) => e.expr.handle_await(),
			syn::Expr::Break(e) => e.expr.handle_await(),
			syn::Expr::Continue(_) => (),
			syn::Expr::Return(e) => e.expr.handle_await(),
			syn::Expr::Macro(e) => {
				if path_is_await(&e.mac.path) {
					let inner_e: syn::Expr = syn::parse(e.mac.tts.clone().into()).unwrap();
					new_expr = Some(build_await(&inner_e));
				}
			},
			syn::Expr::Struct(e) => {
				for field in e.fields.iter_mut() {
					field.expr.handle_await();
				}
				e.rest.handle_await();
			},
			syn::Expr::Repeat(e) => e.expr.handle_await(), // ignore e.len, should be a const/literal
			syn::Expr::Paren(e) => e.expr.handle_await(),
			syn::Expr::Group(e) => e.expr.handle_await(),
			syn::Expr::Try(e) => e.expr.handle_await(),
			// syn::Expr::Async(e) => e.block.handle_await(), // async { ... } ?
			syn::Expr::Async(_) => panic!("async blocks not supported"),
			syn::Expr::TryBlock(e) => e.block.handle_await(),
			syn::Expr::Yield(e) => {
				assert!(e.expr.is_none(), "yield in #[coasync] doesn't take a value");
				new_expr = Some(build_yield());
			},
			syn::Expr::Verbatim(_) => panic!("verbatim expressions not allowed"),
		}

		if let Some(new_expr) = new_expr {
			*self = new_expr;
		}
	}
}

/// Makes a function async, i.e. convert return type to `impl
/// Future<Output = ...>`.
///
/// Within the function body `await!(...)` is supported to wait for
/// futures, and `yield` schedules the task again before yielding (i.e.
/// returning `Pending`).
#[proc_macro_attribute]
pub fn coasync(_args: TokenStream, input: TokenStream) -> TokenStream {
	let mut f = parse_macro_input!(input as ItemFn);

	let output = match f.decl.output.clone() {
		syn::ReturnType::Default => quote! {()},
		syn::ReturnType::Type(_, t) => quote! {#t},
	};

	f.decl.output = match syn::parse((quote! {
		-> impl core::future::Future<Output = #output>
	}).into(),
	) {
		Ok(v) => v,
		Err(e) => return e.to_compile_error().into(),
	};

	f.handle_await();
	let oblock = f.block;

	// println!("new inner code: {}", oblock.clone().into_token_stream());

	let waker = waker_ident();

	f.block = match syn::parse(
		(quote! {{
			use core::task::LocalWaker;
			use core::cell::RefCell;
			use cofutures_inner::WakerContext;

			// delay creation of generator until we are pinned and have a WakerContext
			let mut l = move |#waker: WakerContext| {
				move || {
					if false { yield }  // make sure to trigger generator creation
					#oblock
				}
			};

			unsafe { cofutures_inner::CoAsync::new(l) }
		}})
		.into(),
	) {
		Ok(v) => v,
		Err(e) => {
			eprintln!("failed to generate: {:?}", e);
			return e.to_compile_error().into();
		},
	};

	f.into_token_stream().into()
}
