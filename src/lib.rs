//! The Ferroc Memory Allocator
//!
//! # Usage
//!
//! Ferroc can be used in many ways by various features. Pick one you prefer:
//!
//! ## The default allocator
//!
//! The simplest use of this crate is to configure it as the default global
//! allocator. Simply add the dependency of this crate, mark an instance of
//! [`Ferroc`] using the `#[global_allocator]` attribute, and done!
//!
//! ```
//! use ferroc::Ferroc;
//!
//! #[global_allocator]
//! static FERROC: Ferroc = Ferroc;
//!
//! let _vec = vec![10; 100];
//! ```
//!
//! Or, if you don't want to use it as the global allocator, [`Ferroc`] provides
//! methods of all the functions you need, as well as the implementation of
//! [`core::alloc::Allocator`]:
//!
//! ```
//! #![feature(allocator_api)]
//! use ferroc::Ferroc;
//!
//! let mut vec = Vec::with_capacity_in(10, Ferroc);
//! vec.extend([1, 2, 3, 4, 5]);
//!
//! let layout = std::alloc::Layout::new::<[u32; 10]>();
//! let memory = Ferroc.allocate(layout).unwrap();
//! unsafe { Ferroc.deallocate(memory.cast(), layout) };
//! ```
//!
//! ## The default allocator (custom configuration)
//!
//! An instance of Ferroc type consists of thread-local contexts and heaps and a
//! global arena collection based on a specified base allocator, which can be
//! configured as you like.
//!
//! To do so, disable the default feature and enable the `global` feature as
//! well as other necessary features to [configure](config) it! Take the
//! embedded use case for example:
//!
//! ```toml
//! [dependencies.ferroc]
//! version = "*"
//! default-features = false
//! features = ["global", "base-static"]
//! ```
//!
//! ```rust,ignore
//! use std::{alloc::Layout, cell::UnsafeCell, ptr::NonNull};
//!
//! use ferroc::base::Static;
//!
//! // This is the capacity of the necessary additional static
//! // memory space used by ferroc as the metadata storage.
//! const HEADER_CAP: usize = 4096;
//! static STATIC: Static<HEADER_CAP> = Static::new();
//!
//! ferroc::config!(pub Custom(&STATIC) => &'static Static::<HEADER_CAP>);
//!
//! #[global_allocator]
//! static CUSTOM: Custom = Custom;
//!
//! // If you want use static memory allocation with the standard library,
//! // be sure to load at least one chunk at initialization time, since
//! // Rust's std allocates memory at start-up as well.
//! #[ctor::ctor]
//! fn load_memory() {
//!     // Every chunk must be aligned at least 4MB.
//!     #[repr(align(4194304))]
//!     struct Memory(UnsafeCell<[usize; 1024]>);
//!     unsafe impl Sync for Memory {}
//!     static STATIC_MEM: Memory = Memory(UnsafeCell::new([0; 1024]));
//!
//!     // Multiple chunks can be loaded at runtime.
//!     let pointer = unsafe { NonNull::new_unchecked(STATIC_MEM.0.get().cast()) };
//!     let chunk = unsafe { Chunk::from_static(pointer, Layout::new::<Memory>()) };
//!     CUSTOM.manage(chunk).unwrap();
//! }
//!
//! let vec = vec![1, 2, 3, 4, 5];
//! assert_eq!(vec.iter().sum::<i32>(), 15);
//! ```
//!
//! ## MORE customizations
//!
//! If the configurations of the default allocator can't satisfy your need, you
//! can use the intermediate structures manually while disabling unnecessary
//! features:
//! ```rust
//! #![feature(allocator_api)]
//! use core::pin::pin;
//! use ferroc::{
//!     arena::Arenas,
//!     heap::{Heap, Context},
//!     base::Mmap,
//! };
//!
//! let arenas = Arenas::new(Mmap); // `Arenas` are `Send` & `Sync`...
//! let cx = pin!(Context::new(&arenas));
//! let heap = Heap::new(cx.as_ref()); // ...while `Context`s and `Heap`s are not.
//!
//! // Using the allocator API.
//! let mut vec = Vec::new_in(&heap);
//! vec.extend([1, 2, 3, 4]);
//! assert_eq!(vec.iter().sum::<i32>(), 10);
//!
//! // Manually allocate memory.
//! let layout = std::alloc::Layout::new::<u8>();
//! let ptr = heap.allocate(layout).unwrap();
//! unsafe { heap.deallocate(ptr.cast(), layout) };
//!
//! // Immediately run some delayed clean-up operations.
//! heap.collect(/* force */false);
//! ```
//!
//! ## Using as a dynamic library for `malloc` function series
//!
//! Simply enable the `c` feature and compile it, and you can retrieve the
//! library binary alongside with a `ferroc.h` C/C++ compatible header.
//!
//! If you want to replace the default `malloc` implementation, the `c-override`
//! feature can be enabled.
#![no_std]
#![warn(missing_docs)]
#![feature(alloc_layout_extra)]
#![feature(allocator_api)]
#![feature(const_pin)]
#![feature(const_refs_to_static)]
#![feature(if_let_guard)]
#![feature(isqrt)]
#![feature(let_chains)]
#![feature(non_null_convenience)]
#![feature(pointer_is_aligned)]
#![feature(ptr_as_uninit)]
#![feature(ptr_mask)]
#![feature(ptr_metadata)]
#![feature(ptr_sub_ptr)]
#![feature(raw_os_error_ty)]
#![feature(strict_provenance)]
#![cfg_attr(feature = "c", feature(linkage))]
#![cfg_attr(feature = "global", allow(internal_features))]
#![cfg_attr(feature = "global", feature(allow_internal_unsafe))]
#![cfg_attr(feature = "global", feature(allow_internal_unstable))]

#[cfg(any(test, miri, feature = "base-mmap"))]
extern crate std;

#[cfg(feature = "c")]
macro_rules! forward {
    (@IMPL $name:ident($($aname:ident, $atype:ty),*) $(-> $ret:ty)? => $target:ident) => {
        #[no_mangle]
        #[cfg(feature = "c-override")]
        pub unsafe extern "C" fn $name($($aname: $atype),*) $(-> $ret)? {
            $target($($aname),*)
        }
    };
    ($($name:ident($($aname:ident: $atype:ty),*) $(-> $ret:ty)? => $target:ident;)*) => {
        $(forward!(@IMPL $name($($aname, $atype),*) $(-> $ret)? => $target);)*
    };
}

pub mod arena;
pub mod base;
#[cfg(feature = "c")]
mod c;
#[cfg(feature = "c")]
mod cpp;
#[cfg(feature = "global")]
#[doc(hidden)]
pub mod global;
pub mod heap;
mod slab;
#[doc(hidden)]
pub mod track;

#[cfg(feature = "default")]
config_mod!(global_mmap: pub crate::base::Mmap: pthread);

#[cfg(feature = "default")]
pub use self::global_mmap::*;

#[cfg(test)]
mod test {
    use core::{iter, pin::pin};
    use std::{thread, vec::Vec};

    use crate::{
        arena::{slab_layout, Arenas, SHARD_SIZE, SLAB_SIZE},
        base::{BaseAlloc, Mmap},
        heap::{Context, Heap},
        Ferroc,
    };

    #[test]
    fn basic() {
        let mut vec = Vec::new_in(Ferroc);
        vec.extend([1, 2, 3, 4]);
        vec.extend([5, 6, 7, 8]);
        assert_eq!(vec.iter().sum::<i32>(), 10 + 26);
        drop(vec);
    }

    const CHUNK_SIZE: usize = 1024;
    fn chunks(size: usize) -> impl Iterator<Item = [u8; CHUNK_SIZE]> {
        iter::repeat([0u8; CHUNK_SIZE]).take(size / CHUNK_SIZE)
    }

    #[test]
    fn large() {
        let mut vec = Vec::new_in(Ferroc);
        vec.extend(chunks(SLAB_SIZE / 2 - SHARD_SIZE));
        vec[2 * SHARD_SIZE / CHUNK_SIZE][0] = 123;
        drop(vec);
        let mut vec = Vec::new_in(Ferroc);
        vec.extend(chunks(SLAB_SIZE * 15 / 32));
        vec[2 * SHARD_SIZE / CHUNK_SIZE][0] = 123;
        drop(vec);
    }

    #[test]
    fn huge() {
        let mut vec = Vec::new_in(Ferroc);
        vec.extend(chunks(SLAB_SIZE + 5 * SHARD_SIZE));
        vec[SLAB_SIZE / 2 / CHUNK_SIZE][0] = 123;
        drop(vec);
    }

    #[test]
    fn manage() {
        let chunk = Ferroc.base().allocate(slab_layout(5), false).unwrap();
        Ferroc.manage(chunk).unwrap();
    }

    #[test]
    fn multithread() {
        let j = thread::spawn(move || {
            let mut vec = Vec::new_in(Ferroc);
            vec.extend(iter::repeat(0u8).take(100));
            vec.extend(iter::repeat(1u8).take(100));
            vec
        });
        let vec = j.join().unwrap();
        drop(vec);
    }

    #[test]
    fn local() {
        let arenas = Arenas::new(Mmap);
        let cx = pin!(Context::new(&arenas));
        let heap = Heap::new(cx.as_ref());

        let mut vec = Vec::new_in(&heap);
        vec.extend(chunks(SLAB_SIZE / 2));
        vec[SHARD_SIZE / CHUNK_SIZE][0] = 123;
        drop(vec);
    }
}
