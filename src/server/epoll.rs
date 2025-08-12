#![cfg(feature = "epoll")]
use super::{ConnectionMeta, RouteFn, Server, StreamSetupAction};
use crate::server::{HandlerConfig, handle_one_request};
use crate::threadpool::{Task, ThreadPool};
use crate::{HttpRouter, ResponseHandle};
use libc::{
    EPOLL_CTL_ADD, EPOLL_CTL_DEL, EPOLL_CTL_MOD, EPOLLET, EPOLLIN, EPOLLONESHOT, epoll_create1,
    epoll_ctl, epoll_event, epoll_wait,
};
use std::io;
use std::net::{TcpListener, TcpStream};
use std::os::unix::io::{AsRawFd, RawFd};
use std::sync::Arc;

struct Connection {
    read_stream: TcpStream,
    response: ResponseHandle,
    meta: ConnectionMeta,
    fd: RawFd,
}

struct EpollJob<R> {
    conn_ptr: u64,
    epfd: RawFd,
    handler_config: Arc<HandlerConfig<R>>,
}

impl<R> Task for EpollJob<R>
where
    R: HttpRouter<Route = Box<RouteFn>> + Send + Sync + 'static,
{
    #[inline]
    fn run(self) {
        unsafe {
            let mut conn = Box::from_raw(self.conn_ptr as *mut Connection);
            conn.meta.increment();

            let keep_alive = handle_one_request(
                &mut conn.read_stream,
                &mut conn.response,
                &self.handler_config,
                &conn.meta,
            )
            .unwrap_or(false);

            if keep_alive {
                let mut ev = epoll_event {
                    events: (EPOLLIN | EPOLLONESHOT | EPOLLET) as u32,
                    u64: self.conn_ptr,
                };
                epoll_ctl(self.epfd, EPOLL_CTL_MOD, conn.fd, &mut ev);
                let _ = Box::into_raw(conn); // leak conn ptr
            } else {
                epoll_ctl(self.epfd, EPOLL_CTL_DEL, conn.fd, std::ptr::null_mut());
                // conn gets dropped here
            }
        }
    }
}

impl<R> Server<R>
where
    R: HttpRouter<Route = Box<RouteFn>> + Send + Sync + 'static,
{
    pub fn serve_epoll(self) -> io::Result<()> {
        let listener = TcpListener::bind(&*self.bind_addrs)?;
        listener.set_nonblocking(true)?;

        let epfd = unsafe { epoll_create1(0) };
        if epfd == -1 {
            return Err(io::Error::last_os_error());
        }

        const LISTENER_PTR: u64 = 1; // pseudo-pointer: 1 is never equal to a real heap address
        let mut ev = epoll_event {
            events: (EPOLLIN | EPOLLET) as u32,
            u64: LISTENER_PTR,
        };
        if unsafe { epoll_ctl(epfd, EPOLL_CTL_ADD, listener.as_raw_fd(), &mut ev) } == -1 {
            return Err(io::Error::last_os_error());
        }

        let pool: ThreadPool<EpollJob<R>> = ThreadPool::new(self.thread_count);
        let max_events = self.epoll_queue_max_events as i32;
        let mut events = vec![epoll_event { events: 0, u64: 0 }; max_events as usize];

        loop {
            let n = unsafe { epoll_wait(epfd, events.as_mut_ptr(), max_events, -1) };
            if n == -1 {
                match io::Error::last_os_error() {
                    e if e.kind() == io::ErrorKind::Interrupted => continue,
                    e => return Err(e),
                }
            }

            for ev in &events[..n as usize] {
                let conn_ptr = ev.u64;

                if conn_ptr == LISTENER_PTR {
                    // listener is nonblocking, so WouldBlock breaks this loop
                    while let Ok((mut stream, _)) = listener.accept() {
                        if let Some(hook) = &self.stream_setup_hook {
                            stream = match (hook)(Ok(stream)) {
                                StreamSetupAction::Proceed(s) => s,
                                StreamSetupAction::Drop => continue,
                                StreamSetupAction::StopAccepting => return Ok(()),
                            }
                        }

                        let fd = stream.as_raw_fd();
                        let write_stream = match stream.try_clone() {
                            Ok(s) => s,
                            Err(_e) => continue, // TODO: how to handle this?
                        };
                        let response = ResponseHandle::new(write_stream);
                        let conn = Box::new(Connection {
                            read_stream: stream,
                            response,
                            meta: ConnectionMeta::new(),
                            fd,
                        });
                        let ptr = Box::into_raw(conn) as u64;
                        let mut ev = epoll_event {
                            events: (EPOLLIN | EPOLLONESHOT | EPOLLET) as u32,
                            u64: ptr,
                        };

                        if unsafe { epoll_ctl(epfd, EPOLL_CTL_ADD, fd, &mut ev) } == -1 {
                            unsafe {
                                drop(Box::from_raw(ptr as *mut Connection));
                            }
                        }
                    }
                } else {
                    pool.execute(EpollJob {
                        conn_ptr,
                        epfd,
                        handler_config: Arc::clone(&self.handler_config),
                    });
                }
            }
        }
    }
}
