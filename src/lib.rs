extern crate futures;
extern crate tokio_core;
extern crate tokio_io;

use futures::{Future, Poll};
use std::time::Duration;
use std::io::{self, Read, Write};
use tokio_core::reactor::{Handle, Timeout};
use tokio_io::{AsyncRead, AsyncWrite};

pub struct TimeoutReader<R> {
    reader: R,
    timeout: Duration,
    handle: Handle,
    cur: Option<Timeout>,
}

impl<R> TimeoutReader<R>
    where R: AsyncRead
{
    pub fn new(reader: R, timeout: Duration, handle: &Handle) -> TimeoutReader<R> {
        TimeoutReader {
            reader,
            timeout,
            handle: handle.clone(),
            cur: None,
        }
    }

    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
        self.cur = None;
    }

    pub fn get_ref(&self) -> &R {
        &self.reader
    }

    pub fn get_mut(&mut self) -> &mut R {
        &mut self.reader
    }

    pub fn into_inner(self) -> R {
        self.reader
    }
}

impl<R> Read for TimeoutReader<R>
    where R: AsyncRead
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut timer = self.cur.take();

        if let Some(ref mut timer) = timer {
            if timer.poll()?.is_ready() {
                return Err(io::Error::from(io::ErrorKind::TimedOut));
            }
        }

        let r = self.reader.read(buf);

        match r {
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                let timer = match timer {
                    Some(timer) => timer,
                    None => Timeout::new(self.timeout, &self.handle)?,
                };
                self.cur = Some(timer);
            }
            _ => {}
        }

        r
    }
}

impl<R> AsyncRead for TimeoutReader<R>
    where R: AsyncRead
{
    unsafe fn prepare_uninitialized_buffer(&self, buf: &mut [u8]) -> bool {
        self.reader.prepare_uninitialized_buffer(buf)
    }
}

impl<R> Write for TimeoutReader<R>
    where R: Write
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.reader.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.reader.flush()
    }
}

impl<R> AsyncWrite for TimeoutReader<R>
    where R: AsyncWrite
{
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        self.reader.shutdown()
    }
}

pub struct TimeoutWriter<W> {
    writer: W,
    timeout: Duration,
    handle: Handle,
    cur: Option<Timeout>,
}

impl<W> TimeoutWriter<W>
    where W: AsyncWrite
{
    pub fn new(writer: W, timeout: Duration, handle: &Handle) -> TimeoutWriter<W> {
        TimeoutWriter {
            writer,
            timeout,
            handle: handle.clone(),
            cur: None,
        }
    }

    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
        self.cur = None;
    }

    pub fn get_ref(&self) -> &W {
        &self.writer
    }

    pub fn get_mut(&mut self) -> &mut W {
        &mut self.writer
    }

    pub fn into_inner(self) -> W {
        self.writer
    }
}

impl<W> Write for TimeoutWriter<W>
    where W: AsyncWrite
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut timer = self.cur.take();

        if let Some(ref mut timer) = timer {
            if timer.poll()?.is_ready() {
                return Err(io::Error::from(io::ErrorKind::TimedOut));
            }
        }

        let r = self.writer.write(buf);

        match r {
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                let timer = match timer {
                    Some(timer) => timer,
                    None => Timeout::new(self.timeout, &self.handle)?,
                };
                self.cur = Some(timer);
            }
            _ => {}
        }

        r
    }

    fn flush(&mut self) -> io::Result<()> {
        // TODO should a timeout be applied here as well?
        self.writer.flush()
    }
}

impl<W> AsyncWrite for TimeoutWriter<W>
    where W: AsyncWrite
{
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        // TODO should a timeout be applied here as well?
        self.writer.shutdown()
    }
}

impl<W> Read for TimeoutWriter<W>
    where W: Read
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.writer.read(buf)
    }
}

impl<W> AsyncRead for TimeoutWriter<W>
    where W: AsyncRead
{
    unsafe fn prepare_uninitialized_buffer(&self, buf: &mut [u8]) -> bool {
        self.writer.prepare_uninitialized_buffer(buf)
    }
}

// TODO this stores two copies of the Handle which is maybe not great?
pub struct TimeoutStream<S>(TimeoutReader<TimeoutWriter<S>>);

impl<S> TimeoutStream<S>
    where S: AsyncRead + AsyncWrite
{
    pub fn new(stream: S,
               read_timeout: Duration,
               write_timeout: Duration,
               handle: &Handle)
               -> TimeoutStream<S> {
        let writer = TimeoutWriter::new(stream, write_timeout, handle);
        let reader = TimeoutReader::new(writer, read_timeout, handle);
        TimeoutStream(reader)
    }

    pub fn set_read_timeout(&mut self, timeout: Duration) {
        self.0.set_timeout(timeout)
    }

    pub fn set_write_timeout(&mut self, timeout: Duration) {
        self.0.get_mut().set_timeout(timeout)
    }

    pub fn get_ref(&self) -> &S {
        self.0.get_ref().get_ref()
    }

    pub fn get_mut(&mut self) -> &mut S {
        self.0.get_mut().get_mut()
    }

    pub fn into_inner(self) -> S {
        self.0.into_inner().into_inner()
    }
}

impl<S> Read for TimeoutStream<S>
    where S: AsyncRead + AsyncWrite
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl<S> AsyncRead for TimeoutStream<S>
    where S: AsyncRead + AsyncWrite
{
    unsafe fn prepare_uninitialized_buffer(&self, buf: &mut [u8]) -> bool {
        self.0.prepare_uninitialized_buffer(buf)
    }
}

impl<S> Write for TimeoutStream<S>
    where S: AsyncRead + AsyncWrite
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl<S> AsyncWrite for TimeoutStream<S>
    where S: AsyncRead + AsyncWrite
{
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        self.0.shutdown()
    }
}

#[cfg(test)]
mod test {
    use futures::Async;
    use tokio_core::reactor::Core;

    use super::*;

    struct DelayStream(Timeout);

    impl Read for DelayStream {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.0.poll()?.is_ready() {
                buf[0] = 0;
                Ok(1)
            } else {
                Err(io::Error::from(io::ErrorKind::WouldBlock))
            }
        }
    }

    impl AsyncRead for DelayStream {
        unsafe fn prepare_uninitialized_buffer(&self, _: &mut [u8]) -> bool {
            true
        }
    }

    impl Write for DelayStream {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            if self.0.poll()?.is_ready() {
                Ok(buf.len())
            } else {
                Err(io::Error::from(io::ErrorKind::WouldBlock))
            }
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl AsyncWrite for DelayStream {
        fn shutdown(&mut self) -> Poll<(), io::Error> {
            Ok(Async::Ready(()))
        }
    }

    struct ReadFuture(TimeoutReader<DelayStream>);

    impl Future for ReadFuture {
        type Item = ();
        type Error = io::Error;

        fn poll(&mut self) -> Poll<(), io::Error> {
            let mut buf = [0; 1];
            match self.0.read(&mut buf) {
                Ok(_) => Ok(Async::Ready(())),
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => Ok(Async::NotReady),
                Err(e) => Err(e),
            }
        }
    }

    #[test]
    fn read_timeout() {
        let mut core = Core::new().unwrap();

        let reader = DelayStream(Timeout::new(Duration::from_millis(500), &core.handle()).unwrap());
        let reader = TimeoutReader::new(reader, Duration::from_millis(100), &core.handle());

        let r = core.run(ReadFuture(reader));
        assert_eq!(r.unwrap_err().kind(), io::ErrorKind::TimedOut);
    }

    #[test]
    fn read_ok() {
        let mut core = Core::new().unwrap();

        let reader = DelayStream(Timeout::new(Duration::from_millis(100), &core.handle()).unwrap());
        let reader = TimeoutReader::new(reader, Duration::from_millis(500), &core.handle());

        core.run(ReadFuture(reader)).unwrap();
    }

    struct WriteFuture(TimeoutWriter<DelayStream>);

    impl Future for WriteFuture {
        type Item = ();
        type Error = io::Error;

        fn poll(&mut self) -> Poll<(), io::Error> {
            match self.0.write(&[0]) {
                Ok(_) => Ok(Async::Ready(())),
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => Ok(Async::NotReady),
                Err(e) => Err(e),
            }
        }
    }

    #[test]
    fn write_timeout() {
        let mut core = Core::new().unwrap();

        let writer = DelayStream(Timeout::new(Duration::from_millis(500), &core.handle()).unwrap());
        let writer = TimeoutWriter::new(writer, Duration::from_millis(100), &core.handle());

        let r = core.run(WriteFuture(writer));
        assert_eq!(r.unwrap_err().kind(), io::ErrorKind::TimedOut);
    }

    #[test]
    fn write_ok() {
        let mut core = Core::new().unwrap();

        let writer = DelayStream(Timeout::new(Duration::from_millis(100), &core.handle()).unwrap());
        let writer = TimeoutWriter::new(writer, Duration::from_millis(500), &core.handle());

        core.run(WriteFuture(writer)).unwrap();
    }
}
