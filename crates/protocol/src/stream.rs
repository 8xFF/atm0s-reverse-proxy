use std::{
    future::Future,
    io,
    pin::Pin,
    task::{ready, Context, Poll},
};

use futures::{AsyncRead, AsyncWrite};

pub fn pipeline_streams<
    S1: AsyncRead + AsyncWrite + NamedStream + Unpin,
    S2: AsyncRead + AsyncWrite + NamedStream + Unpin,
>(
    a: S1,
    b: S2,
) -> BidirectCopyStream<S1, S2> {
    log::info!(
        "Process bidirect-copy between {} <==> {}",
        a.name(),
        b.name()
    );
    BidirectCopyStream {
        a,
        b,
        a_to_b: OneDirectState::Running(CopyBuffer::default()),
        b_to_a: OneDirectState::Running(CopyBuffer::default()),
    }
}

pub trait NamedStream {
    fn name(&self) -> &'static str;
}

const BUFFER_SIZE: usize = 4096;
const BUFFER_WRITE_MIN: usize = 100;

#[derive(Clone)]
struct CopyBuffer {
    buf: [u8; BUFFER_SIZE],
    front_pos: usize,
    back_pos: usize,
    sum: usize,
    lock_write: bool,
}

impl Default for CopyBuffer {
    fn default() -> Self {
        Self {
            buf: [0; BUFFER_SIZE],
            front_pos: 0,
            back_pos: 0,
            sum: 0,
            lock_write: false,
        }
    }
}

impl CopyBuffer {
    fn lock_write(&mut self) {
        self.lock_write = true;
    }

    fn is_lock_write(&self) -> bool {
        self.lock_write
    }

    fn front(&self) -> &[u8] {
        &self.buf[self.front_pos..self.back_pos]
    }

    pub fn move_front(&mut self, len: usize) {
        assert!(
            self.front_pos + len <= self.back_pos,
            "should not read more data than it"
        );
        self.front_pos += len;
        self.sum += len;

        if self.front_pos == self.back_pos {
            self.front_pos = 0;
            self.back_pos = 0;
        }
    }

    fn back_mut(&mut self) -> &mut [u8] {
        if self.lock_write {
            //if in lock_write then we return empty buf
            &mut self.buf[..0]
        } else {
            &mut self.buf[self.back_pos..]
        }
    }

    fn move_back(&mut self, len: usize) {
        assert!(
            self.back_pos + len <= self.buf.len(),
            "should not append more data than space"
        );
        self.back_pos += len;
        self.sum += len;
    }
}

enum OneDirectState {
    Running(CopyBuffer),
    ShuttingDown(usize),
    Done(usize),
}

fn process_one_direct<F, T>(
    cx: &mut Context<'_>,
    from: &mut F,
    to: &mut T,
    state: &mut OneDirectState,
) -> Poll<io::Result<usize>>
where
    F: AsyncRead + AsyncWrite + NamedStream + Unpin,
    T: AsyncRead + AsyncWrite + NamedStream + Unpin,
{
    let mut from = Pin::new(from);
    let mut to = Pin::new(to);

    loop {
        match state {
            OneDirectState::Running(copy_buffer) => {
                let write_buf = copy_buffer.back_mut();
                if write_buf.len() >= BUFFER_WRITE_MIN {
                    //only write if we have big enough space
                    match from.as_mut().poll_read(cx, write_buf).map_err(|e| {
                        log::error!(
                            "[OneDirectCopy {} => {}] read error {e:?}",
                            from.name(),
                            to.name()
                        );
                        e
                    })? {
                        Poll::Ready(0) => {
                            log::info!(
                                "[OneDirectCopy {} => {}] read finished => lock_write",
                                from.name(),
                                to.name()
                            );
                            //read stream closed => lock write
                            copy_buffer.lock_write();
                        }
                        Poll::Ready(len) => {
                            copy_buffer.move_back(len);
                            log::debug!(
                                "[OneDirectCopy {} => {}] read {len} bytes, current buf {} bytes",
                                from.name(),
                                to.name(),
                                copy_buffer.front().len()
                            );
                        }
                        Poll::Pending => {}
                    }
                }

                let read_buf = copy_buffer.front();
                if read_buf.len() > 0 {
                    // we have some data to write
                    match to.as_mut().poll_write(cx, read_buf).map_err(|e| {
                        log::error!(
                            "[OneDirectCopy {} => {}] write error {e:?}",
                            from.name(),
                            to.name()
                        );
                        e
                    })? {
                        Poll::Ready(len) => {
                            copy_buffer.move_front(len);
                            log::debug!(
                                "[OneDirectCopy {} => {}] write {len} bytes, current buf {} bytes",
                                from.name(),
                                to.name(),
                                copy_buffer.front().len()
                            );
                        }
                        Poll::Pending => {}
                    }
                } else if copy_buffer.is_lock_write() {
                    // we finish then switch to ShutingDonw
                    log::info!(
                        "[OneDirectCopy {} => {}] write finished after {} bytes => switch to ShuttingDown state",
                        from.name(),
                        to.name(),
                        copy_buffer.sum
                    );
                    *state = OneDirectState::ShuttingDown(copy_buffer.sum)
                } else {
                    return Poll::Pending;
                }
            }
            OneDirectState::ShuttingDown(sum) => {
                ready!(to.as_mut().poll_flush(cx)).map_err(|e| {
                    log::error!(
                        "[OneDirectCopy {} => {}] flush error {e:?}",
                        from.name(),
                        to.name()
                    );
                    e
                })?;
                log::info!(
                    "[OneDirectCopy {} => {}] write finished flush",
                    from.name(),
                    to.name(),
                );
                ready!(to.as_mut().poll_close(cx)).map_err(|e| {
                    log::error!(
                        "[OneDirectCopy {} => {}] close error {e:?}",
                        from.name(),
                        to.name()
                    );
                    e
                })?;
                log::info!(
                    "[OneDirectCopy {} => {}] write finished close => Done",
                    from.name(),
                    to.name(),
                );
                *state = OneDirectState::Done(*sum)
            }
            OneDirectState::Done(sum) => return Poll::Ready(Ok(*sum)),
        }
    }
}

pub struct BidirectCopyStream<S1, S2> {
    a: S1,
    b: S2,
    a_to_b: OneDirectState,
    b_to_a: OneDirectState,
}

impl<
        S1: AsyncRead + AsyncWrite + NamedStream + Unpin,
        S2: AsyncRead + AsyncWrite + NamedStream + Unpin,
    > Future for BidirectCopyStream<S1, S2>
{
    type Output = Result<(usize, usize), std::io::Error>;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();

        let res1 = process_one_direct(cx, &mut this.a, &mut this.b, &mut this.a_to_b)?;
        let res2 = process_one_direct(cx, &mut this.b, &mut this.a, &mut this.b_to_a)?;

        let a_to_b = ready!(res1);
        let b_to_a = ready!(res2);

        log::info!(
            "[BidirectCopyStream] {} <==> {} finished",
            this.a.name(),
            this.b.name()
        );

        Poll::Ready(Ok((a_to_b, b_to_a)))
    }
}
