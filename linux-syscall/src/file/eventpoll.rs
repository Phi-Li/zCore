use super::*;
use alloc::boxed::Box;
use alloc::{vec::Vec, collections::BTreeMap, collections::BTreeSet};
use bitvec::prelude::{BitVec, Lsb0};
use core::future::Future;
use core::mem::{replace, size_of};
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

    /// Open an eventpoll file descriptor.
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

    /// Implement the controller interface for the eventpoll file that enables
    /// the insertion/removal/change of file descriptors inside the interest set.
    pub fn sys_epoll_ctl(
        &mut self,
        epfd: FileDesc,
        op: usize,
        fd: usize,
        event_p: UserInPtr<EpollEvent>
    ) -> SysResult {
        let proc = self.linux_process();


        let ep = proc.get_file_like(epfd).unwrap();
 
        /// Get the "struct file *" for the target file
        let target_f = proc.get_file_like(fd).unwrap();

        /// The target file descriptor must support poll
        // if !UNIMPLEMENTED_file_can_poll(target_f) {
        //     return Err(LxError::EPERM);
        // }

        /// Check if EPOLLWAKEUP is allowed

        /// We have to check that the file structure underneath the file descriptor
        /// the user passed to us _is_ an eventpoll file. And also we do not permit
        /// adding an epoll file descriptor inside itself.
        if ep == target_f || !UNIMPLEMENTED_is_file_epoll(f) {
            return Err(LxError::EINVAL)
        }

        /// When we insert an epoll file descriptor inside another epoll file
        /// descriptor, there is the chance of creating closed loops, which are
        /// better be handled here, than in more critical paths.

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

    /// Implement the event wait interface for the eventpoll file.
    /// It is the kernel part of the user space epoll_wait(2).
    pub async fn sys_epoll_wait(
        &mut self,
        epfd: FileDesc,
        events: UserInOutPtr<EpollEvent>,
        maxevents: isize,
        timeout: usize
    ) -> SysResult {
        /// The maximum number of event must be greater than zero
        if maxevents <= 0 || maxevents > EP_MAX_EVENTS {
            return Err(LxError::EINVAL);
        }

        #[must_use = "future does nothing unless polled/`await`-ed"]
        struct EpollFuture<'a> {
            syscall: &'a Syscall<'a>,
            events: Vec<EpollEvent>,
            time_now_ms: usize,
            timeout_ms: usize,
        }

        let now = ;

        impl<'a> Future for EpollFuture<'a> {
            type Output = LxResult<Vec<EpollEvent>>;

            fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                let proc = self.syscall.linux_process();

                /// Get the eventpoll instance by epfd
                /// or return EBADF
                let ep = proc.get_file_like(epfd)?;

                /// Time to fish for events ...
                ep.poll()
            }
        }

        let evs = EpollFuture {
            syscall: self,
            events: Vec::new(),
            time_now_ms: TimeVal::now().to_msec(),
            timeout_ms: timeout,
        }.await?;
        events.write_array(evs);
        Ok(evs.len())
    }

    pub async fn sys_epoll_pwait(
        &mut self,
        epfd: FileDesc,
        events: UserInOutPtr<EpollEvent>,
        maxevents: isize,
        timeout: usize,
        sigmask: usize
    ) -> SysResult {
        /// If the caller wants a certain signal mask to be set during the wait,
        /// we apply it here.

        self.sys_epoll_wait(epfd, events, maxevents, timeout).await
    }

    pub async fn sys_epoll_pwait2(
        &mut self,
        epfd: FileDesc,
        events: UserInOutPtr<EpollEvent>,
        maxevents: isize,
        timeout: UserInPtr<TimeSpec>,
        sigmask: usize
    ) -> SysResult {
        if timeout.is_null() {

        } else {
            self.sys_epoll_pwait(epfd, events, maxevents, timeout.read().unwrap().to_msec(), sigmask).await
        }
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
const EPOLLET: u32 = 1 << 31;

/// Epoll private bits inside the event mask
const EP_PRIVATE_BITS: u32 = EPOLLWAKEUP | EPOLLONESHOT | EPOLLET | EPOLLEXCLUSIVE;

const EPOLLINOUT_BITS: EpollEventMask = EpollEventMask::EPOLLIN | EpollEventMask::EPOLLOUT;

const EPOLLEXCLUSIVE_OK_BITS: u32 = EPOLLINOUT_BITS | EpollEventMask::EPOLLERR | EpollEventMask::EPOLLHUP | EPOLLWAKEUP | EPOLLET | EPOLLEXCLUSIVE;

/// Maximum number of nesting allowed inside epoll sets
const EP_MAX_NESTS: u8 = 4;

const EP_MAX_EVENTS: usize = i32::MAX / size_of(EpollEvent);

const EP_ITEM_COST: usize = size_of(EpItem);

/// Configuration options available inside /proc/sys/fs/epoll/
/// Maximum number of epoll watched descriptors, per user
/// Allows top 4% of lomem to be allocated for epoll watches (per user).
/// max_user_watches = (((si.totalram - si.totalhigh) / 25) << PAGE_SHIFT) / EP_ITEM_COST;
/// This should be set dynamically, here we set a fixed value for simplicity.
static max_user_watches: usize = 1048576;

#[derive(Debug)]
#[repr(C)]
pub struct EpollEvent {
    events: EpollEventMask,
    data: u64,
}

/// This structure implements EpollFile trait and represents
/// the main data structure for the eventpoll interface.
struct EventPoll {
    /// UNIMPLEMENTED: ready_list is a subset of tree,
    /// they should share references to EpItem.
    /// The following is just a concise representation.

    /// List of ready file descriptors
    ready_list: Vec<EpItem>,
    /// BTreeMap used to store monitored fd structs
    tree: BTreeMap<FileDesc, EpItem>,
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

    /// Removes a EpItem from the eventpoll BTreeMap
    fn remove(&mut self, fd: FileDesc) {
        self.tree.remove(fd);
    }

    /// Search the file inside the eventpoll tree.
    fn find_mut(&mut self, fd: FileDesc) -> Option<&mut EpItem> {
        self.tree.get_mut(fd)
    }

    fn insert(&mut self, event: EpollEvent, tfile: Arc<dyn FileLike>, fd: FileDesc) {
        /// Detect nested epoll instance
        // let tep: EventPoll;
        // if UNIMPLEMENTED_is_file_epoll(tfile) {
        //     tep = tfile.UNIMPLEMENTED_private_data;
        // }

        /// UNIMPLEMENTED: check max_user_watches

        /// EpItem initialization follow here ...
        let epi = EpItem::new(EpollFileFd::new(tfile, fd), event);

        /// Add the current item to the BtreeMap.
        self.tree.insert(fd, epi);

        /// Get current event bits.
        let revents = epi.poll();

        /// If the file is already "ready" we drop it inside the ready list
        if revents != 0 {
            self.ready_list.push(epi);

            /// UNIMPLEMENTED: Notify waiting tasks that events are available
        }
    }

    fn scan(&mut self, maxevents: usize) -> Vec<EpollEvent> {
        let events = Vec::<EpollEvent>::new();

        /// Steal the ready list, and re-init the original one to the empty list.
        let txlist = replace(self.ready_list, Vec::<EpItem>::new());

        for epi in txlist {
            /// If the event mask intersect the caller-requested one,
            /// deliver the event to userspace.
            let revents = epi.poll();
            if revents == 0 {
                continue;
            }

            events.push(EpItem::new(epi.event.data, revents));

            if epi.event.events & EPOLLONESHOT {
                epi.event.events &= EP_PRIVATE_BITS;
            } else if epi.event.events & EPOLLET == 0 {
                /// If this file has been added with Level Trigger mode,
                /// we need to insert back inside the ready list,
                /// so that the next call to epoll_wait() will
                /// check again the events availability.
                self.ready_list.push(epi);
            }
        }

        events
    }

    /// Retrieves ready events.
    ///
    /// @maxevents: Size (in terms of number of events) of the caller event buffer.
    /// @timeout: Maximum timeout for the ready events fetch operation, in timespec.
    ///
    /// Return: Ready events which have been fetched, or an error code, in case of error.
    ///
    fn poll(&mut self, maxevents: usize, timed_out: usize) -> LxResult<Vec<EpollEvent>> {
        if !self.ready_list.is_empty() {
            /// Try to transfer events to user space.
            /// In case we get 0 events and there's still timeout left over,
            /// we go trying again in search of more luck.
            let res = self.scan(maxevents);
            if !res.is_empty() {
                return Ok(res);
            }
        }
    }
}

/// The eventpoll file behaviour is different from normal file
pub trait EpollFile {
    /// For /proc file system
    fn show_fdinfo();
    fn release();
    fn poll();
    fn llseek();
}

impl EpollFile for EventPoll {

}

/// Each file descriptor added to the eventpoll interface will
/// have an entry of this type in the BTreeMap.
#[derive(Clone, Copy)]
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
        let file = self.file_fd.file;
        let res = file.poll().unwrap();

        UNIMPLEMENTED_cast_PollStatus_into_bitflags(res) & self.event.events
    }

    /// Modify the interest event mask by dropping an event if the new mask
    /// has a match in the current file status.
    fn modify(&mut self, event: EpollEvent) {

        /// Set the new event interest mask before polling fd
        self.event = event;

        /// UNIMPLEMENTED: EPOLLWAKEUP flag

        /// Get current event bits.
        let ev = self.poll();
        /// If the item is "hot" and it is not registered inside the ready
        /// list, push it inside.
    }
}

impl PartialEq for EpItem {
    fn eq(&self, other: &Self) -> bool {
        self.file_fd == other.file_fd
    }
}

impl Eq for EpItem {}

impl PartialOrd for EpItem {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.file_fd.partial_cmp(&other.file_fd)
    }
}

impl Ord for EpItem {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.file_fd.cmp(&other.file_fd)
    }
}

/// Rust BTreeMap doesn't allow a custom compare function for searching,
/// so we need to use the search key separately.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
struct EpollFileFd {
    file: Arc<dyn FileLike>,
    fd: FileDesc,
}

impl EpollFileFd {
    /// Setup the structure that is used as key for the BTreeMap
    fn new(file: Arc<dyn FileLike>, fd: FileDesc) -> EpollFileFd {
        EpollFileFd {
            // file: UNIMPLEMENTED_File,
            fd: fd,
        }
    }
}

/// The original epoll implementation uses a combination of
/// the fd number and the file struct pointer to distinguish
/// epoll items, but trait FileLike doesn't require Ord
/// so Arc<dyn FileLike> can't act as the file struct pointer.
/// We just use the fd number here.

impl PartialEq for EpollFileFd {
    fn eq(&self, other: &Self) -> bool {
        self.fd == other.fd
    }
}

impl Eq for EpollFileFd {}

impl PartialOrd for EpollFileFd {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.fd.partial_cmp(&other.fd)
    }
}

impl Ord for EpollFileFd {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.fd.cmp(&other.fd)
    }
}
