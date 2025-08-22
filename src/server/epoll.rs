#![cfg(feature = "epoll")]
#[cfg(all(
    feature = "epoll",
    not(all(target_os = "linux", target_pointer_width = "64"))
))]
compile_error!("feature `epoll` requires Linux on a 64-bit target.");

use super::{ConnectionMeta, Server, StreamSetupAction};
use crate::server::{handle_one_request, HandlerConfig};
use crate::threadpool::{Task, ThreadPool};
use crate::ResponseHandle;

use libc::{
    epoll_create1, epoll_ctl, epoll_event, epoll_wait, eventfd, write, EFD_CLOEXEC, EFD_NONBLOCK,
    EPOLLET, EPOLLIN, EPOLLRDHUP, EPOLL_CTL_ADD, EPOLL_CTL_DEL,
};
use std::mem::size_of;
use std::net::{TcpListener, TcpStream};
use std::os::unix::io::{AsRawFd, RawFd};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::{io, mem};

struct Connection {
    read_stream: TcpStream,
    response: ResponseHandle,
    meta: ConnectionMeta,
}

#[repr(u8)]
#[derive(Copy, Clone, PartialEq)]
enum ConnState {
    Open = 0,
    Closing = 1,
    Closed = 2,
}

#[inline]
fn load_state(a: &AtomicU8, order: Ordering) -> ConnState {
    match a.load(order) {
        0 => ConnState::Open,
        1 => ConnState::Closing,
        _ => ConnState::Closed,
    }
}

#[inline]
fn store_state(a: &AtomicU8, s: ConnState, order: Ordering) {
    a.store(s as u8, order);
}

#[repr(align(64))]
struct Handle {
    in_flight: AtomicBool, // ensure only one worker processes this connection at a time
    state: AtomicU8,       // ConnState
    ptr: *mut Connection,
    fd: RawFd,
}

type CloseQueue = Mutex<Vec<u64>>;

struct EpollJob {
    handle_ptr: u64,             // *mut Handle as u64
    handler_config_ptr_u64: u64, // *const HandlerConfig as u64
    closeq_ptr_u64: u64,         // *const CloseQueue as u64
    efd: RawFd,
}

impl Task for EpollJob {
    #[inline(always)]
    fn run(self) {
        unsafe {
            let handle = &*(self.handle_ptr as *mut Handle);
            let handler_config = &*(self.handler_config_ptr_u64 as *const HandlerConfig);
            let closeq = &*(self.closeq_ptr_u64 as *const CloseQueue);
            let conn = &mut *handle.ptr;

            conn.meta.increment();
            let keep_alive = handle_one_request(
                &mut conn.read_stream,
                &mut conn.response,
                handler_config,
                &conn.meta,
            )
            .unwrap_or(false);

            if keep_alive {
                handle.in_flight.store(false, Ordering::Release);
            } else {
                store_state(&handle.state, ConnState::Closing, Ordering::Release);
                {
                    let mut q = closeq.lock().unwrap();
                    q.push(self.handle_ptr);
                }
                let _ = write(self.efd, (&1u64 as *const u64).cast(), size_of::<u64>());
                handle.in_flight.store(false, Ordering::Release);
            }
        }
    }
}

impl Server {
    pub fn serve_epoll(self) -> io::Result<()> {
        // Tokens used in epoll_event.u64 (never equal to real heap addresses)
        const LISTENER_TOKEN: u64 = 1;
        const EVENTFD_TOKEN: u64 = 2;

        let (listener, epfd) = self.create_listener(LISTENER_TOKEN)?;
        let efd = Self::create_eventfd(EVENTFD_TOKEN, epfd)?;
        let worker_pool: ThreadPool<EpollJob> = ThreadPool::new(self.thread_count);
        let mut pending_free: Vec<u64> = Vec::new();

        let closeq: Arc<CloseQueue> = Arc::new(Mutex::new(Vec::new()));
        let closeq_ptr_u64 = Arc::as_ptr(&closeq) as u64;
        let handler_cfg_ptr_u64 = Arc::as_ptr(&self.handler_config) as u64;

        let max_events = self.epoll_queue_max_events as i32;
        let mut events = vec![epoll_event { events: 0, u64: 0 }; max_events as usize];

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
                        if let Some(hook) = &self.stream_setup_hook {
                            stream = match (hook)(Ok(stream)) {
                                StreamSetupAction::Proceed(s) => s,
                                StreamSetupAction::Drop => continue,
                                StreamSetupAction::StopAccepting => return Ok(()),
                            }
                        }

                        let _ = stream.set_nodelay(true);
                        let write_stream = match stream.try_clone() {
                            Ok(s) => s,
                            Err(_) => continue, // probably out of FDs
                        };
                        let fd = stream.as_raw_fd();

                        let conn = Box::new(Connection {
                            read_stream: stream,
                            response: ResponseHandle::new(write_stream),
                            meta: ConnectionMeta::new(),
                        });
                        let conn_ptr = Box::into_raw(conn);

                        let handle = Box::new(Handle {
                            in_flight: AtomicBool::new(false),
                            state: AtomicU8::new(ConnState::Open as u8),
                            ptr: conn_ptr,
                            fd,
                        });
                        let handle_ptr = Box::into_raw(handle) as u64;

                        let mut cev = epoll_event {
                            events: (EPOLLIN | EPOLLRDHUP) as u32,
                            u64: handle_ptr,
                        };
                        if unsafe { epoll_ctl(epfd, EPOLL_CTL_ADD, fd, &mut cev) } == -1 {
                            unsafe {
                                drop(Box::from_raw(conn_ptr));
                                drop(Box::from_raw(handle_ptr as *mut Handle));
                            }
                        }
                    }
                } else if token == EVENTFD_TOKEN {
                    Self::drain_eventfd(efd, &mut pending_free, &closeq);
                } else {
                    let handle = unsafe { &*(token as *mut Handle) };

                    if load_state(&handle.state, Ordering::Relaxed) != ConnState::Open {
                        continue;
                    }

                    if !handle.in_flight.swap(true, Ordering::AcqRel) {
                        worker_pool.execute(EpollJob {
                            handle_ptr: token,
                            handler_config_ptr_u64: handler_cfg_ptr_u64,
                            closeq_ptr_u64,
                            efd,
                        });
                    }
                }
            }

            if !pending_free.is_empty() {
                Self::finalize_pending(epfd, &mut pending_free);
            }
        }
    }

    fn finalize_pending(epfd: RawFd, pending_free: &mut Vec<u64>) {
        let mut i = 0;
        while i < pending_free.len() {
            let handle_ptr_u64 = pending_free[i];
            let handle = unsafe { &*(handle_ptr_u64 as *mut Handle) };

            // if still in-flight, defer
            if handle.in_flight.load(Ordering::Acquire) {
                i += 1;
                continue;
            }

            // best-effort
            let _ = unsafe { epoll_ctl(epfd, EPOLL_CTL_DEL, handle.fd, std::ptr::null_mut()) };

            // free up resources
            let prev = load_state(&handle.state, Ordering::Acquire);
            if prev != ConnState::Closed {
                store_state(&handle.state, ConnState::Closed, Ordering::Release);
                unsafe {
                    drop(Box::from_raw(handle.ptr)); // free Connection first
                    drop(Box::from_raw(handle_ptr_u64 as *mut Handle));
                }
            }

            pending_free.swap_remove(i); // i now points to next element
        }
    }

    fn drain_eventfd(efd: i32, pending: &mut Vec<u64>, close_queue: &CloseQueue) {
        let mut z: u64 = 0;
        loop {
            let n = unsafe { libc::read(efd, (&mut z as *mut u64).cast(), mem::size_of::<u64>()) };
            if n == -1 && io::Error::last_os_error().raw_os_error().unwrap_or(0) == libc::EINTR {
                continue;
            }
            break;
        }
        let batch = {
            let mut q = close_queue.lock().unwrap();
            mem::take(&mut *q)
        };
        pending.extend(batch);
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

    fn create_eventfd(event_fd_token: u64, epfd: i32) -> io::Result<i32> {
        let efd = unsafe { eventfd(0, EFD_NONBLOCK | EFD_CLOEXEC) };
        if efd < 0 {
            return Err(io::Error::last_os_error());
        }
        let mut eev = epoll_event {
            events: (EPOLLIN | EPOLLET) as u32,
            u64: event_fd_token,
        };
        if unsafe { epoll_ctl(epfd, EPOLL_CTL_ADD, efd, &mut eev) } == -1 {
            return Err(io::Error::last_os_error());
        }
        Ok(efd)
    }
}
