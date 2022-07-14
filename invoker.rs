use crate::sdk;

use std::{
	ffi::c_void,
	sync::atomic::{AtomicPtr, Ordering},
};

use fnv::FnvHashMap;
use once_cell::sync::Lazy;

const fn create_invoker_context() -> InvokerContext {
	InvokerContext {
		context: sdk::scrNativeCallContext {
			results: std::ptr::null_mut(),
			arg_count: 0,
			arguments: std::ptr::null_mut(),
			data_count: 0,
			data: [0; 0xC0],
		},
		stack: [0; 64],
	}
}

static mut INVOKER_CONTEXT: InvokerContext = create_invoker_context();

#[repr(C)]
pub struct InvokerContext {
	pub context: sdk::scrNativeCallContext,
	pub stack: [sdk::scrNativeValue; 64],
}

impl InvokerContext {
	fn init_base(&mut self) {
		self.context.results = self.stack.as_mut_ptr();
		self.context.arguments = self.stack.as_mut_ptr();
	}

	#[inline(always)]
	pub fn init(&mut self) {
		self.context.arg_count = 0;
		self.context.data_count = 0;
	}

	#[inline(always)]
	pub fn push_argument<T>(&mut self, value: T) {
		let index = self.context.arg_count;

		if std::mem::size_of_val(&value) < std::mem::size_of_val(&self.stack[index as usize]) {
			self.stack[index as usize] = 0;
		}

		unsafe {
			*(self.stack.as_mut_ptr().add(index as usize) as *mut T) = value;
		}

		self.context.arg_count += 1;
	}

	#[inline(never)]
	fn invoke_with_context(hash: sdk::scrNativeHash, context: &mut sdk::scrNativeCallContext) {
		if let Some(handler) = get_native_handler(hash) {
			handler(context);
		} else {
			log::debug!("Failed to find handler for native {:#X}.", hash);
		}

		context.set_data_results();
	}

	#[inline(always)]
	pub fn invoke<T: Copy>(&mut self, hash: sdk::scrNativeHash) -> T {
		Self::invoke_with_context(hash, &mut self.context);
		unsafe { *(self.stack.as_ptr() as *const T) }
	}

	#[inline(always)]
	pub fn get() -> &'static mut Self {
		unsafe { &mut INVOKER_CONTEXT }
	}
}

static mut NATIVE_CACHE: Lazy<FnvHashMap<sdk::scrNativeHash, sdk::scrNativeHandler>> =
	Lazy::new(FnvHashMap::default);

pub fn get_native_handler(hash: sdk::scrNativeHash) -> Option<sdk::scrNativeHandler> {
	let native_cache = unsafe { &NATIVE_CACHE };

	native_cache.get(&hash).copied()
}

pub fn set_native_handler(hash: sdk::scrNativeHash, handler: sdk::scrNativeHandler) {
	let native_cache = unsafe { &mut NATIVE_CACHE };
	native_cache.insert(hash, handler);
}

#[inline(never)]
fn register_native_handlers() {
	log::debug!("Registering natives...");

	let native_cache = unsafe { &mut NATIVE_CACHE };

	let native_registration_table =
		unsafe { &*(sdk::NATIVE_REGISTRATION_TABLE.load(Ordering::Relaxed)) };

	for (old_hash, new_hash) in sdk::CROSSMAP.iter() {
		if let Some(handler) = native_registration_table.get_native_handler(*new_hash) {
			native_cache.insert(*old_hash, handler);
		}
	}

	log::debug!("Registered {} natives.", native_cache.len());
}

pub fn init() {
	InvokerContext::get().init_base();
	register_native_handlers();
}

pub static INVOKER_TRAMPOLINE: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
