use super::*;
use alloc::boxed::Box;
use alloc::{vec::Vec, collections::BTreeMap, collections::BTreeSet};
use bitvec::prelude::{BitVec, Lsb0};
use core::future::Future;
use core::mem::size_of;
use core::pin::Pin;
use core::task::{Context, Poll};
use core::time::Duration;
use kernel_hal::timer_set;
use linux_object::fs::{File, FileDesc};
use linux_object::time::*;


impl Syscall<'_> {
    pub fn sys_epoll_create(&mut self) -> SysResult {
        self.sys_epoll_create1(0)
    }

    pub fn sys_epoll_create1(&mut self, flags: usize) -> SysResult {
        let proc = self.linux_process();


        if flags & !EPOLL_CLOEXEC {
            return Err(LxError::EINVAL);
        }

        /// Create the internal data structure ("struct EventPoll").
        let ep = EventPoll::new();

        /// Creates all the items needed to setup an eventpoll file.
        /// That is, a file structure and a free file descriptor.
        let fd = proc.add_file(ep).unwrap();
 
        Ok(fd)
    }

    pub fn sys_epoll_ctl(
        &mut self,
        epfd: FileDesc,
        op: usize,
        fd: usize,
        event_p: UserInPtr<EpollEvent>
    ) -> SysResult {
        let proc = self.linux_process();


        let ep_f = proc.get_file_like(epfd).unwrap();
 
        /// Get the "struct file *" for the target file
        let target_f = proc.get_file_like(fd).unwrap();

        /// The target file descriptor must support poll
        if !UNIMPLEMENTED_file_can_poll(target_f) {
            return Err(LxError::EPERM);
        }

        /// We have to check that the file structure underneath the file descriptor
        /// the user passed to us _is_ an eventpoll file. And also we do not permit
        /// adding an epoll file descriptor inside itself.
        if ep_f == target_f || !UNIMPLEMENTED_is_file_epoll(f) {
            return Err(LxError::EINVAL)
        }

        let ep: EventPoll = ep_f.UNIMPLEMENTED_private_data;

        /// Try to lookup the file inside BTreeMap.
        let epi = ep.find_mut(target_f, fd);

        match op as u8 {
            OpCodes::EPOLL_CTL_ADD => {
                match epi {
                    None => {
                        event_p.as_ptr().events |= EpollEventMask::EPOLLERR | EpollEventMask::EPOLLHUP;
                        ep.insert(event_p.into(), tfile, fd);
                    }
                    Some(_) => return Err(LxError::EEXIST)
                }
            }
            OpCodes::EPOLL_CTL_DEL => {
                match epi {
                    Some(epi) => {
                        ep.remove(epi.file_fd);
                    }
                    None => return Err(LxError::ENOENT)
                }
            }
            OpCodes::EPOLL_CTL_MOD => {
                match epi {
                    Some(epi) => {
                        if !(epi.event.events & EPOLLEXCLUSIVE) {
                            event_p.as_ptr().events |= EpollEventMask::EPOLLERR | EpollEventMask::EPOLLHUP;
                            ep.modify(epi, event_p.into());
                        }
                    }
                    None => return Err(LxError::ENOENT)
                }
            }
        }

        Ok(0)
    }

    pub async fn sys_epoll_wait(
        epfd: usize,
        events: UserInOutPtr<Vec<EpollEvent>>,
        maxevents: usize,
        timeout: usize
    ) -> SysResult {

    }

    pub async fn sys_epoll_pwait(
        epfd: usize,
        events: UserInOutPtr<Vec<EpollEvent>>,
        maxevents: usize,
        timeout: usize,
        sigmask: usize
    ) -> SysResult {

    }

    pub async fn sys_epoll_pwait2(
        epfd: usize,
        events: UserInOutPtr<Vec<EpollEvent>>,
        maxevents: usize,
        timeout: UserInPtr<TimeSpec>,
        sigmask: usize
    ) -> SysResult {

    }
}

/// Flags for epoll_create1.
const EPOLL_CLOEXEC: usize = FileFlags::O_CLOEXEC;

/// Valid opcodes to issue to sys_epoll_ctl()
bitflags! {
    pub struct OpCodes: u8 {
        const EPOLL_CTL_ADD = 1;
        const EPOLL_CTL_DEL = 2;
        const EPOLL_CTL_MOD = 3;
    }
}

/// Epoll event masks
bitflags! {
    pub struct EpollEventMask: u32 {
        const EPOLLIN       = 0x00000001;
        const EPOLLPRI      = 0x00000002;
        const EPOLLOUT      = 0x00000004;
        const EPOLLERR      = 0x00000008;
        const EPOLLHUP      = 0x00000010;
        const EPOLLNVAL     = 0x00000020;
        const EPOLLRDNORM   = 0x00000040;
        const EPOLLRDBAND   = 0x00000080;
        const EPOLLWRNORM   = 0x00000100;
        const EPOLLWRBAND   = 0x00000200;
        const EPOLLMSG      = 0x00000400;
        const EPOLLRDHUP    = 0x00002000;
    }
}

/// Set exclusive wakeup mode for the target file descriptor
const EPOLLEXCLUSIVE: u32 = 1 << 28;

/// See epoll(7) and epoll_ctl(2)
const EPOLLWAKEUP: u32 = 1 << 29;

/// Set the One Shot behaviour for the target file descriptor
const EPOLLONESHOT: u32 = 1 << 30;

/// Set the Edge Triggered behaviour for the target file descriptor
const EPOLLONESHOT: u32 = 1 << 31;

#[derive(Debug)]
#[repr(C)]
pub struct EpollEvent {
    events: EpollEventMask,
    data: u64,
}

struct EventPoll {
    /// UNIMPLEMENTED: ready_list is a subset of tree,
    /// they should share references to EpItem.
    /// The following is just a concise representation.

    /// List of ready file descriptors
    ready_list: Vec<EpItem>,
    /// BTreeMap used to store monitored fd structs
    tree: BTreeMap<EpollFileFd, EpItem>,
    /// The user that created the eventpoll descriptor
    user: usize,
}

impl EventPoll {
    fn new() -> EventPoll {
        EventPoll {
            ready_list: Vec::new(),
            tree: BTreeMap::new(),
            user: 0,
        }
    }

    fn remove(&mut self, file_fd: EpollFileFd) {
        self.tree.remove(file_fd);
    }

    fn find_mut(&mut self, file: UNIMPLEMENTED_File, fd: usize) -> Option<&mut EpItem> {
        self.tree.get_mut(EpollFileFd::new(file, fd))
    }

    fn insert(&mut self, event: EpollEvent, tfile: UNIMPLEMENTED_File, fd: usize) {
        let tep: EventPoll;
        if UNIMPLEMENTED_is_file_epoll(tfile) {
            tep = tfile.UNIMPLEMENTED_private_data;
        }

        /// UNIMPLEMENTED: check max_user_watches

        /// EpItem initialization follow here ...
        let epi = EpItem::new(EpollFileFd::new(tfile, fd), event);

        /// Add the current item to the BtreeMap.
        self.tree.insert(EpollFileFd::new(tfile, fd), epi);

        /// Get current event bits.
        let revents = epi.poll();

        /// If the file is already "ready" we drop it inside the ready list
        if revents != 0 {
            self.ready_list.push(epi);

            /// UNIMPLEMENTED: Notify waiting tasks that events are available
        }
    }
}

impl FileLike for EventPoll {

}

struct EpItem {
    /// The file descriptor information this item refers to
    file_fd: EpollFileFd,
    /// The structure that describe the interested events and the source fd
    event: EpollEvent,
}

impl EpItem {
    fn new(file_fd: EpollFileFd, event: EpollEvent) -> EpItem {
        EpItem {
            file_fd,
            event
        }
    }

    fn poll(&self) -> EpollEventMask {
        // let file = self.file_fd.file;
        let file: File;
        let res = file.poll().unwrap();

        /// ISSUE: Can PollStatus be cast into bit mask safely?
        res & self.event.events
    }

    fn modify(&mut self, event: EpollEvent) {

        /// Set the new event interest mask before polling fd
        self.event = event;

        /// UNIMPLEMENTED: EPOLLWAKEUP flag
    }
}

impl Eq for EpItem {

}

impl PartialOrd for EpItem {

}

impl Ord for EpItem {

}

/// Rust BTreeMap doesn't allow a custom compare function for searching,
/// so we need to use the search key separately.
#[derive(Debug)]
#[repr(C)]
struct EpollFileFd {
    file: UNIMPLEMENTED_File,
    fd: usize,
}

impl EpollFileFd {
    /// Setup the structure that is used as key for the BtreeMap
    fn new(file: UNIMPLEMENTED_File, fd: usize) -> EpollFileFd {
        EpollFileFd {
            file: UNIMPLEMENTED_File,
            fd: fd
        }
    }
}

impl Eq for EpollFileFd {

}

impl PartialOrd for EpollFileFd {

}

impl Ord for EpollFileFd {

}
