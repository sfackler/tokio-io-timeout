//! Tokio wrappers which apply timeouts to IO operations.
//!
//! These timeouts are analagous to the read and write timeouts on traditional
//! blocking sockets. A timeout countdown is initiated when a read/write
//! operation returns `WouldBlock`. If a read/write does not return successfully
//! the before the countdown expires, `TimedOut` is returned.
#![doc(html_root_url = "https://docs.rs/tokio-io-timeout/0.3")]
#![warn(missing_docs)]
extern crate futures;
extern crate tokio_io;
extern crate tokio_timer;

#[cfg(test)]
extern crate tokio;

use bytes::{Buf, BufMut};
use futures::{Future, task::Poll};
use std::{io, pin::Pin, task::Context};
use std::time::{Duration, Instant};
use tokio_io::{AsyncRead, AsyncWrite};
use tokio_timer::{delay, Delay};

#[derive(Debug)]
struct TimeoutState {
    timeout: Option<Duration>,
    cur: Delay,
    active: bool,
}

impl TimeoutState {
    #[inline]
    fn new() -> TimeoutState {
        TimeoutState {
            timeout: None,
            cur: delay(Instant::now()),
            active: false,
        }
    }

    #[inline]
    fn timeout(&self) -> Option<Duration> {
        self.timeout
    }

    #[inline]
    fn set_timeout(&mut self, timeout: Option<Duration>) {
        self.timeout = timeout;
        self.reset();
    }

    #[inline]
    fn reset(&mut self) {
        if self.active {
            self.active = false;
            self.cur.reset(Instant::now());
        }
    }

    #[inline]
    fn poll_check(&mut self, cx: &mut Context) -> io::Result<()> {
        let timeout = match self.timeout {
            Some(timeout) => timeout,
            None => return Ok(()),
        };

        if !self.active {
            self.cur.reset(Instant::now() + timeout);
            self.active = true;
        }

        match Pin::new(&mut self.cur).poll(cx) {
            Poll::Ready(()) => Err(io::Error::from(io::ErrorKind::TimedOut)),
            Poll::Pending => Ok(()),
        }
    }
}

/// An `AsyncRead`er which applies a timeout to read operations.
#[derive(Debug)]
pub struct TimeoutReader<R> {
    reader: R,
    state: TimeoutState,
}

impl<R> TimeoutReader<R>
where
    R: AsyncRead + Unpin,
{
    /// Returns a new `TimeoutReader` wrapping the specified reader.
    ///
    /// There is initially no timeout.
    pub fn new(reader: R) -> TimeoutReader<R> {
        TimeoutReader {
            reader,
            state: TimeoutState::new(),
        }
    }

    /// Returns the current read timeout.
    pub fn timeout(&self) -> Option<Duration> {
        self.state.timeout()
    }

    /// Sets the read timeout.
    ///
    /// This will reset any pending timeout.
    pub fn set_timeout(&mut self, timeout: Option<Duration>) {
        self.state.set_timeout(timeout);
    }

    /// Returns a shared reference to the inner reader.
    pub fn get_ref(&self) -> &R {
        &self.reader
    }

    /// Returns a mutable reference to the inner reader.
    pub fn get_mut(&mut self) -> &mut R {
        &mut self.reader
    }

    /// Consumes the `TimeoutReader`, returning the inner reader.
    pub fn into_inner(self) -> R {
        self.reader
    }
}

impl<R> AsyncRead for TimeoutReader<R>
where
    R: AsyncRead + Unpin,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut [u8],
    ) -> Poll<Result<usize, io::Error>> {
        let s = self.get_mut();
        let r = Pin::new(&mut s.reader).poll_read(cx, buf);
        match r {
            Poll::Pending => s.state.poll_check(cx)?,
            _ => s.state.reset(),
        }
        r
    }

    unsafe fn prepare_uninitialized_buffer(&self, buf: &mut[u8]) -> bool {
        self.reader.prepare_uninitialized_buffer(buf)
    }

    fn poll_read_buf<B>(
        self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut B,
    ) -> Poll<Result<usize, io::Error>>
    where
        B: BufMut,
    {
        let s = self.get_mut();
        let r = Pin::new(&mut s.reader).poll_read_buf(cx, buf);
        match r {
            Poll::Pending => s.state.poll_check(cx)?,
            _ => s.state.reset(),
        }
        r
    }
}

impl<R> AsyncWrite for TimeoutReader<R>
where
    R: AsyncWrite + Unpin,
{
    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut Context,
    ) -> Poll<Result<(), io::Error>> {
        Pin::new(&mut self.get_mut().reader).poll_shutdown(cx)
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut Context,
    ) -> Poll<Result<(), io::Error>> {
        Pin::new(&mut self.get_mut().reader).poll_flush(cx)
    }

    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        Pin::new(&mut self.get_mut().reader).poll_write(cx, buf)
    }

    fn poll_write_buf<B>(
        self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut B
    ) -> Poll<Result<usize, io::Error>>
    where
        B: Buf,
    {
        let s = self.get_mut();
        Pin::new(&mut s.reader).poll_write_buf(cx, buf)
    }
}

/// An `AsyncWrite`er which applies a timeout to write operations.
#[derive(Debug)]
pub struct TimeoutWriter<W> {
    writer: W,
    state: TimeoutState,
}

impl<W> TimeoutWriter<W>
where
    W: AsyncWrite,
{
    /// Returns a new `TimeoutReader` wrapping the specified reader.
    ///
    /// There is initially no timeout.
    pub fn new(writer: W) -> TimeoutWriter<W> {
        TimeoutWriter {
            writer,
            state: TimeoutState::new(),
        }
    }

    /// Returns the current write timeout.
    pub fn timeout(&self) -> Option<Duration> {
        self.state.timeout()
    }

    /// Sets the write timeout.
    ///
    /// This will reset any pending timeout.
    pub fn set_timeout(&mut self, timeout: Option<Duration>) {
        self.state.set_timeout(timeout);
    }

    /// Returns a shared reference to the inner writer.
    pub fn get_ref(&self) -> &W {
        &self.writer
    }

    /// Returns a mutable reference to the inner writer.
    pub fn get_mut(&mut self) -> &mut W {
        &mut self.writer
    }

    /// Consumes the `TimeoutWriter`, returning the inner writer.
    pub fn into_inner(self) -> W {
        self.writer
    }
}

impl<W> AsyncWrite for TimeoutWriter<W>
where
    W: AsyncWrite + Unpin,
{
    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut Context,
    ) -> Poll<Result<(), io::Error>> {
        let s = self.get_mut();
        let r = Pin::new(&mut s.writer).poll_shutdown(cx);
        match r {
            Poll::Pending => s.state.poll_check(cx)?,
            _ => s.state.reset(),
        }
        r
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut Context,
    ) -> Poll<Result<(), io::Error>> {
        let s = self.get_mut();
        let r = Pin::new(&mut s.writer).poll_flush(cx);
        match r {
            Poll::Pending => s.state.poll_check(cx)?,
            _ => s.state.reset(),
        }
        r
    }

    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        let s = self.get_mut();
        let r = Pin::new(&mut s.writer).poll_write(cx, buf);
        match r {
            Poll::Pending => s.state.poll_check(cx)?,
            _ => s.state.reset(),
        }
        r
    }

    fn poll_write_buf<B>(
        self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut B
    ) -> Poll<Result<usize, io::Error>>
    where
        B: Buf,
    {
        let s = self.get_mut();
        let r = Pin::new(&mut s.writer).poll_write_buf(cx, buf);
        match r {
            Poll::Pending => s.state.poll_check(cx)?,
            _ => s.state.reset(),
        }
        r
    }
}

impl<W> AsyncRead for TimeoutWriter<W>
where
    W: AsyncRead + Unpin,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut [u8],
    ) -> Poll<Result<usize, io::Error>> {
        Pin::new(&mut self.get_mut().writer).poll_read(cx, buf)
    }

    unsafe fn prepare_uninitialized_buffer(&self, buf: &mut[u8]) -> bool {
        return self.writer.prepare_uninitialized_buffer(buf);
    }

    fn poll_read_buf<B>(
        self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut B,
    ) -> Poll<Result<usize, io::Error>>
    where
        B: BufMut,
    {
        let s = self.get_mut();
        Pin::new(&mut s.writer).poll_read_buf(cx, buf)
    }
}

/// A stream which applies read and write timeouts to an inner stream.
#[derive(Debug)]
pub struct TimeoutStream<S>(TimeoutReader<TimeoutWriter<S>>);

impl<S> TimeoutStream<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    /// Returns a new `TimeoutStream` wrapping the specified stream.
    ///
    /// There is initially no read or write timeout.
    pub fn new(stream: S) -> TimeoutStream<S> {
        let writer = TimeoutWriter::new(stream);
        let reader = TimeoutReader::new(writer);
        TimeoutStream(reader)
    }

    /// Returns the current read timeout.
    pub fn read_timeout(&self) -> Option<Duration> {
        self.0.timeout()
    }

    /// Sets the read timeout.
    ///
    /// This will reset any pending read timeout.
    pub fn set_read_timeout(&mut self, timeout: Option<Duration>) {
        self.0.set_timeout(timeout)
    }

    /// Returns the current write timeout.
    pub fn write_timeout(&self) -> Option<Duration> {
        self.0.get_ref().timeout()
    }

    /// Sets the write timeout.
    ///
    /// This will reset any pending write timeout.
    pub fn set_write_timeout(&mut self, timeout: Option<Duration>) {
        self.0.get_mut().set_timeout(timeout)
    }

    /// Returns a shared reference to the inner stream.
    pub fn get_ref(&self) -> &S {
        self.0.get_ref().get_ref()
    }

    /// Returns a mutable reference to the inner stream.
    pub fn get_mut(&mut self) -> &mut S {
        self.0.get_mut().get_mut()
    }

    /// Consumes the stream, returning the inner stream.
    pub fn into_inner(self) -> S {
        self.0.into_inner().into_inner()
    }
}

impl<S> AsyncRead for TimeoutStream<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut [u8],
    ) -> Poll<Result<usize, io::Error>> {
        Pin::new(&mut self.get_mut().0).poll_read(cx, buf)
    }

    unsafe fn prepare_uninitialized_buffer(&self, buf: &mut[u8]) -> bool {
        return self.0.prepare_uninitialized_buffer(buf);
    }

    fn poll_read_buf<B>(
        self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut B,
    ) -> Poll<Result<usize, io::Error>>
    where
        B: BufMut,
    {
        let s = self.get_mut();
        Pin::new(&mut s.0).poll_read_buf(cx, buf)
    }
}

impl<S> AsyncWrite for TimeoutStream<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut Context,
    ) -> Poll<Result<(), io::Error>> {
        Pin::new(&mut self.get_mut().0).poll_shutdown(cx)
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut Context,
    ) -> Poll<Result<(), io::Error>> {
        Pin::new(&mut self.get_mut().0).poll_flush(cx)
    }

    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        Pin::new(&mut self.get_mut().0).poll_write(cx, buf)
    }

    fn poll_write_buf<B>(
        self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut B,
    ) -> Poll<Result<usize, io::Error>>
    where
        B: Buf,
    {
        let s = self.get_mut();
        Pin::new(&mut s.0).poll_write_buf(cx, buf)
    }
}

#[cfg(test)]
mod test {
    use futures::{task::Poll, future::Future};
    use std::io::Write;
    use std::net::TcpListener;
    use std::thread;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    use super::*;

    struct DelayStream(Delay);

    impl AsyncRead for DelayStream {
        fn poll_read(
            self: Pin<&mut Self>,
            cx: &mut Context,
            _buf: &mut [u8],
        ) -> Poll<Result<usize, io::Error>> {
            let s = self.get_mut();
            match Pin::new(&mut s.0).poll(cx) {
                Poll::Ready(()) => Poll::Ready(Ok(1)),
                Poll::Pending => Poll::Pending,
            }
        }
    }

    impl AsyncWrite for DelayStream {
        fn poll_shutdown(
            self: Pin<&mut Self>,
            _cx: &mut Context,
        ) -> Poll<Result<(), io::Error>> {
            Poll::Ready(Ok(()))
        }

        fn poll_flush(
            self: Pin<&mut Self>,
            _cx: &mut Context,
        ) -> Poll<Result<(), io::Error>> {
            Poll::Ready(Ok(()))
        }

        fn poll_write(
            self: Pin<&mut Self>,
            cx: &mut Context,
            buf: &[u8],
        ) -> Poll<Result<usize, io::Error>> {
            let s = self.get_mut();
            match Pin::new(&mut s.0).poll(cx) {
                Poll::Ready(()) => Poll::Ready(Ok(buf.len())),
                Poll::Pending => Poll::Pending,
            }
        }
    }

    async fn read_one<R>(mut reader: R) -> Result<usize, io::Error>
    where
        R: AsyncRead + Unpin,
    {
        let mut buf: [u8; 1] = [0; 1];
        reader.read(&mut buf).await
    }

    #[tokio::test]
    async fn read_timeout() {
        let reader = DelayStream(delay(Instant::now() + Duration::from_millis(500)));
        let mut reader = TimeoutReader::new(reader);
        reader.set_timeout(Some(Duration::from_millis(100)));

        let r = read_one(reader).await;
        assert_eq!(r.err().unwrap().kind(), io::ErrorKind::TimedOut);
    }

    #[tokio::test]
    async fn read_ok() {
        let reader = DelayStream(delay(Instant::now() + Duration::from_millis(100)));
        let mut reader = TimeoutReader::new(reader);
        reader.set_timeout(Some(Duration::from_millis(500)));

        read_one(reader).await.unwrap();
    }

    async fn write_one<W>(mut writer: W) -> Result<(), io::Error>
    where
        W: AsyncWrite + Unpin,
    {
        writer.write_all(&[0]).await
    }

    #[tokio::test]
    async fn write_timeout() {
        let writer = DelayStream(delay(Instant::now() + Duration::from_millis(500)));
        let mut writer = TimeoutWriter::new(writer);
        writer.set_timeout(Some(Duration::from_millis(100)));

        let r = write_one(writer).await;
        assert_eq!(r.err().unwrap().kind(), io::ErrorKind::TimedOut);
    }

    #[tokio::test]
    async fn write_ok() {
        let writer = DelayStream(delay(Instant::now() + Duration::from_millis(100)));
        let mut writer = TimeoutWriter::new(writer);
        writer.set_timeout(Some(Duration::from_millis(500)));

        write_one(writer).await.unwrap();
    }

    #[tokio::test]
    async fn tcp_read() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        thread::spawn(move || {
            let mut socket = listener.accept().unwrap().0;
            thread::sleep(Duration::from_millis(10));
            socket.write_all(b"f").unwrap();
            thread::sleep(Duration::from_millis(500));
            let _ = socket.write_all(b"f"); // this may hit an eof
        });

        let s = TcpStream::connect(&addr).await.unwrap();
        let mut s = TimeoutStream::new(s);
        s.set_read_timeout(Some(Duration::from_millis(100)));
        let _ = read_one(&mut s).await.unwrap();
        let r = read_one(&mut s).await;

        match r {
            Ok(_) => panic!("unexpected success"),
            Err(ref e) if e.kind() == io::ErrorKind::TimedOut => (),
            Err(e) => panic!("{:?}", e),
        }
    }
}
