//! Copied from the standard library, with some private methods made public to
//! allow us to fill the buffer up to capacity even when it's not empty.

use std::cmp;
use std::io::{self, BorrowedBuf, Read};
use std::mem::MaybeUninit;

const DEFAULT_BUF_SIZE: usize = 8192;

pub struct BufReader<R: ?Sized> {
    pub buf: Buffer,
    pub inner: R,
}

impl<R: Read> BufReader<R> {
    /// Creates a new `BufReader<R>` with a default buffer capacity. The default is currently 8 KiB,
    /// but may change in the future.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::io::BufReader;
    /// use std::fs::File;
    ///
    /// fn main() -> std::io::Result<()> {
    ///     let f = File::open("log.txt")?;
    ///     let reader = BufReader::new(f);
    ///     Ok(())
    /// }
    /// ```
    pub fn new(inner: R) -> BufReader<R> {
        BufReader::with_capacity(DEFAULT_BUF_SIZE, inner)
    }

    /// Creates a new `BufReader<R>` with the specified buffer capacity.
    ///
    /// # Examples
    ///
    /// Creating a buffer with ten bytes of capacity:
    ///
    /// ```no_run
    /// use std::io::BufReader;
    /// use std::fs::File;
    ///
    /// fn main() -> std::io::Result<()> {
    ///     let f = File::open("log.txt")?;
    ///     let reader = BufReader::with_capacity(10, f);
    ///     Ok(())
    /// }
    /// ```
    pub fn with_capacity(capacity: usize, inner: R) -> BufReader<R> {
        BufReader {
            inner,
            buf: Buffer::with_capacity(capacity),
        }
    }

    pub fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.buf.fill_buf(&mut self.inner)
    }

    pub fn consume(&mut self, amt: usize) {
        self.buf.consume(amt)
    }
}

pub struct Buffer {
    // The buffer.
    buf: Box<[MaybeUninit<u8>]>,
    // The current seek offset into `buf`, must always be <= `filled`.
    pos: usize,
    // Each call to `fill_buf` sets `filled` to indicate how many bytes at the start of `buf` are
    // initialized with bytes from a read.
    filled: usize,
    // This is the max number of bytes returned across all `fill_buf` calls. We track this so that we
    // can accurately tell `read_buf` how many bytes of buf are initialized, to bypass as much of its
    // defensive initialization as possible. Note that while this often the same as `filled`, it
    // doesn't need to be. Calls to `fill_buf` are not required to actually fill the buffer, and
    // omitting this is a huge perf regression for `Read` impls that do not.
    initialized: usize,
}

impl Buffer {
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        let buf = Box::new_uninit_slice(capacity);
        Self {
            buf,
            pos: 0,
            filled: 0,
            initialized: 0,
        }
    }

    #[inline]
    pub fn buffer(&self) -> &[u8] {
        // SAFETY: self.pos and self.cap are valid, and self.cap => self.pos, and
        // that region is initialized because those are all invariants of this type.
        unsafe { MaybeUninit::slice_assume_init_ref(self.buf.get_unchecked(self.pos..self.filled)) }
    }

    #[inline]
    pub fn consume(&mut self, amt: usize) {
        self.pos = cmp::min(self.pos + amt, self.filled);
    }

    /// Read more bytes into the buffer without discarding any of its contents
    pub fn read_more(&mut self, mut reader: impl Read) -> io::Result<usize> {
        let mut buf = BorrowedBuf::from(&mut self.buf[self.filled..]);
        let old_init = self.initialized - self.filled;
        unsafe {
            buf.set_init(old_init);
        }
        reader.read_buf(buf.unfilled())?;
        self.filled += buf.len();
        self.initialized += buf.init_len() - old_init;
        Ok(buf.len())
    }

    /// Remove bytes that have already been read from the buffer.
    pub fn backshift(&mut self) {
        self.buf.copy_within(self.pos..self.filled, 0);
        self.filled -= self.pos;
        self.pos = 0;
    }

    #[inline]
    pub fn fill_buf(&mut self, mut reader: impl Read) -> io::Result<&[u8]> {
        // If we've reached the end of our internal buffer then we need to fetch
        // some more data from the reader.
        // Branch using `>=` instead of the more correct `==`
        // to tell the compiler that the pos..cap slice is always valid.
        if self.pos >= self.filled {
            debug_assert!(self.pos == self.filled);

            let mut buf = BorrowedBuf::from(&mut *self.buf);
            // SAFETY: `self.filled` bytes will always have been initialized.
            unsafe {
                buf.set_init(self.initialized);
            }

            let result = reader.read_buf(buf.unfilled());

            self.pos = 0;
            self.filled = buf.len();
            self.initialized = buf.init_len();

            result?;
        }
        Ok(self.buffer())
    }
}
