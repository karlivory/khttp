#![cfg(feature = "epoll")]
#[cfg(all(
    feature = "epoll",
    not(all(target_os = "linux", target_pointer_width = "64"))
))]
compile_error!("feature `epoll` requires Linux on a 64-bit target.");

use super::{ConnectionSetupAction, Server};
use crate::server::{handle_one_request, HandlerConfig};
use crate::threadpool::{Task, ThreadPool};
use crate::ResponseHandle;

use libc::{
    epoll_create1, epoll_ctl, epoll_event, epoll_wait, EPOLLET, EPOLLIN, EPOLLRDHUP, EPOLL_CTL_ADD,
    EPOLL_CTL_DEL,
};
use std::net::{TcpListener, TcpStream};
use std::os::unix::io::{AsRawFd, RawFd};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::{io, ptr};

#[repr(align(64))]
struct Handle {
    in_flight: AtomicBool, // ensure only one worker processes this connection at a time
    stream_ptr: *mut TcpStream,
    handler_config: Arc<HandlerConfig>,
    fd: RawFd,
    epfd: RawFd,
    closed: AtomicBool,
}

struct EpollJob {
    handle_ptr: u64, // *mut Handle as u64
}

impl Task for EpollJob {
    #[inline(always)]
    fn run(self) {
        let handle = unsafe { &*(self.handle_ptr as *const Handle) };
        let stream = unsafe { &*(handle.stream_ptr) };

        let mut response = ResponseHandle::new(stream);
        let keep_alive =
            handle_one_request(stream, &mut response, &handle.handler_config).unwrap_or(false);

        if keep_alive {
            handle.in_flight.store(false, Ordering::Release);
        } else {
            unsafe {
                let _ = epoll_ctl(handle.epfd, EPOLL_CTL_DEL, handle.fd, ptr::null_mut());
                drop(Box::from_raw(handle.stream_ptr)); // close connection
            }
            handle.closed.store(true, Ordering::Release);
        }
    }
}

impl Server {
    pub fn serve_epoll(self) -> io::Result<()> {
        // Tokens used in epoll_event.u64 (never equal to real heap addresses)
        const LISTENER_TOKEN: u64 = 1;

        let (listener, epfd) = self.create_listener(LISTENER_TOKEN)?;
        let worker_pool: ThreadPool<EpollJob> = ThreadPool::new(self.thread_count);

        let max_events = self.epoll_queue_max_events as i32;
        let mut events = vec![epoll_event { events: 0, u64: 0 }; max_events as usize];
        let mut stale_ptrs = Vec::with_capacity(self.epoll_queue_max_events.max(512));

        loop {
            let n = unsafe { epoll_wait(epfd, events.as_mut_ptr(), max_events, -1) };
            if n == -1 {
                match io::Error::last_os_error() {
                    e if e.kind() == io::ErrorKind::Interrupted => continue,
                    e => return Err(e), // any other `epoll_wait` error is fatal
                }
            }

            for ev in &events[..n as usize] {
                let token = ev.u64;

                if token == LISTENER_TOKEN {
                    // Edge-triggered accept: drain until WouldBlock
                    while let Ok((mut stream, _peer)) = listener.accept() {
                        if let Some(hook) = &self.connection_setup_hook {
                            stream = match (hook)(Ok((stream, _peer))) {
                                ConnectionSetupAction::Proceed(s) => s,
                                ConnectionSetupAction::Drop => continue,
                                ConnectionSetupAction::StopAccepting => return Ok(()),
                            }
                        }

                        let _ = stream.set_nodelay(true);
                        let fd = stream.as_raw_fd();
                        let stream_ptr = Box::into_raw(Box::new(stream));

                        let handle = Box::new(Handle {
                            in_flight: AtomicBool::new(false),
                            handler_config: Arc::clone(&self.handler_config),
                            stream_ptr,
                            epfd,
                            fd,
                            closed: AtomicBool::new(false),
                        });
                        let handle_ptr = Box::into_raw(handle) as u64;

                        let mut cev = epoll_event {
                            events: (EPOLLIN | EPOLLRDHUP) as u32,
                            u64: handle_ptr,
                        };
                        if unsafe { epoll_ctl(epfd, EPOLL_CTL_ADD, fd, &mut cev) } == -1 {
                            unsafe {
                                drop(Box::from_raw(handle_ptr as *mut Handle));
                            }
                        }
                    }
                } else {
                    let handle_ptr = token as *mut Handle;
                    let handle = unsafe { &*handle_ptr };
                    if handle.closed.load(Ordering::Acquire) {
                        stale_ptrs.push(handle_ptr);
                    } else if handle
                        .in_flight
                        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
                        .is_ok()
                    {
                        worker_pool.execute(EpollJob { handle_ptr: token });
                    }
                }
            }
            if !stale_ptrs.is_empty() {
                for ptr in &stale_ptrs {
                    unsafe { drop(Box::from_raw(*ptr)) };
                }
                stale_ptrs.clear();
            }
        }
    }

    fn create_listener(&self, listener_token: u64) -> io::Result<(TcpListener, i32)> {
        let listener = TcpListener::bind(&*self.bind_addrs)?;
        listener.set_nonblocking(true)?;

        let epfd = unsafe { epoll_create1(0) };
        if epfd == -1 {
            return Err(io::Error::last_os_error());
        }
        let mut lev = epoll_event {
            events: (EPOLLIN | EPOLLET) as u32,
            u64: listener_token,
        };
        if unsafe { epoll_ctl(epfd, EPOLL_CTL_ADD, listener.as_raw_fd(), &mut lev) } == -1 {
            return Err(io::Error::last_os_error());
        }
        Ok((listener, epfd))
    }
}
