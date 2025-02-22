//! Linux LibOS
//! - run process and manage trap/interrupt/syscall
#![no_std]
#![feature(asm)]
#![deny(warnings, unused_must_use, missing_docs)]

extern crate alloc;
#[macro_use]
extern crate log;

use {
    alloc::{boxed::Box, string::String, sync::Arc, vec::Vec},
    core::{future::Future, pin::Pin},
    kernel_hal::MMUFlags,
    linux_object::{
        fs::{vfs::FileSystem, INodeExt},
        loader::LinuxElfLoader,
        process::ProcessExt,
        thread::ThreadExt,
    },
    linux_syscall::Syscall,
    zircon_object::task::*,
};

#[cfg(target_arch = "riscv64")]
use {kernel_hal::UserContext, zircon_object::object::KernelObject};

#[cfg(target_arch = "x86_64")]
use kernel_hal::GeneralRegs;

/// Create and run main Linux process
pub fn run(args: Vec<String>, envs: Vec<String>, rootfs: Arc<dyn FileSystem>) -> Arc<Process> {
    let job = Job::root();
    let proc = Process::create_linux(&job, rootfs.clone()).unwrap();
    let thread = Thread::create_linux(&proc).unwrap();
    let loader = LinuxElfLoader {
        #[cfg(feature = "std")]
        syscall_entry: kernel_hal_unix::syscall_entry as usize,
        #[cfg(not(feature = "std"))]
        syscall_entry: 0,
        stack_pages: 8,
        root_inode: rootfs.root_inode(),
    };

    {
        let mut id = 0;
        let rust_dir = rootfs.root_inode().lookup("/").unwrap();
        trace!("run(), Rootfs: / ");
        while let Ok(name) = rust_dir.get_entry(id) {
            id += 1;
            trace!("  {}", name);
        }
    }
    info!("args {:?}", args);
    let inode = rootfs.root_inode().lookup(&args[0]).unwrap();
    let data = inode.read_as_vec().unwrap();
    let path = args[0].clone();
    debug!("Linux process: {:?}", path);

    let pg_token = kernel_hal::current_page_table();
    debug!("current pgt = {:#x}", pg_token);
    //调用zircon-object/src/task/thread.start设置好要执行的thread
    let (entry, sp) = loader.load(&proc.vmar(), &data, args, envs, path).unwrap();

    thread
        .start(entry, sp, 0, 0, thread_fn)
        .expect("failed to start main thread");
    proc
}

/// The function of a new thread.
///
/// loop:
/// - wait for the thread to be ready
/// - get user thread context
/// - enter user mode
/// - handle trap/interrupt/syscall according to the return value
/// - return the context to the user thread
async fn new_thread(thread: CurrentThread) {
    loop {
        // wait
        let mut cx = thread.wait_for_run().await;
        if thread.state() == ThreadState::Dying {
            break;
        }

        // super_mode_net_test();

        // run
        trace!("go to user: {:#x?}", cx);
        kernel_hal::context_run(&mut cx);
        trace!("back from user: {:#x?}", cx);
        // handle trap/interrupt/syscall

        #[cfg(target_arch = "x86_64")]
        match cx.trap_num {
            0x100 => handle_syscall(&thread, &mut cx.general).await,
            0x20..=0x3f => {
                kernel_hal::InterruptManager::handle_irq(cx.trap_num as u32);
                if cx.trap_num == 0x20 {
                    kernel_hal::yield_now().await;
                }
            }
            0xe => {
                let vaddr = kernel_hal::fetch_fault_vaddr();
                let flags = if cx.error_code & 0x2 == 0 {
                    MMUFlags::READ
                } else {
                    MMUFlags::WRITE
                };
                error!("page fualt from user mode {:#x} {:#x?}", vaddr, flags);
                let vmar = thread.proc().vmar();
                match vmar.handle_page_fault(vaddr, flags) {
                    Ok(()) => {}
                    Err(_) => {
                        panic!("Page Fault from user mode {:#x?}", cx);
                    }
                }
            }
            _ => panic!("not supported interrupt from user mode. {:#x?}", cx),
        }

        // UserContext
        #[cfg(target_arch = "riscv64")]
        {
            let trap_num = kernel_hal::fetch_trap_num(&cx);
            let is_interrupt = ((trap_num >> 63) & 1) == 1;
            let trap_num = trap_num & 0xfff;
            let pid = thread.proc().id();
            if is_interrupt {
                match trap_num {
                    //Irq
                    0 | 4 | 5 | 8 | 9 => {
                        kernel_hal::InterruptManager::handle_irq(trap_num as u32);

                        //Timer
                        if trap_num == 4 || trap_num == 5 {
                            debug!("Timer interrupt: {}", trap_num);

                            /*
                            * 已在irq_handle里加入了timer处理函数
                            kernel_hal::timer_set_next();
                            kernel_hal::timer_tick();
                            */

                            kernel_hal::yield_now().await;
                        }

                        //kernel_hal::InterruptManager::handle_irq(trap_num as u32);
                    }
                    _ => panic!(
                        "not supported pid: {} interrupt {} from user mode. {:#x?}",
                        pid, trap_num, cx
                    ),
                }
            } else {
                match trap_num {
                    // syscall
                    8 => handle_syscall(&thread, &mut cx).await,
                    // PageFault
                    12 | 13 | 15 => {
                        let vaddr = kernel_hal::fetch_fault_vaddr();

                        //注意这里flags没有包含WRITE权限，后面handle会移除写权限
                        let flags = if trap_num == 15 {
                            MMUFlags::WRITE
                        } else if trap_num == 12 {
                            MMUFlags::EXECUTE
                        } else {
                            MMUFlags::READ
                        };

                        info!(
                            "page fualt from pid: {} user mode, vaddr:{:#x}, trap:{}",
                            pid, vaddr, trap_num
                        );
                        let vmar = thread.proc().vmar();
                        match vmar.handle_page_fault(vaddr, flags) {
                            Ok(()) => {}
                            Err(error) => {
                                panic!("{:?} Page Fault from user mode {:#x?}", error, cx);
                            }
                        }
                    }
                    _ => panic!(
                        "not supported pid: {} exception {} from user mode. {:#x?}",
                        pid, trap_num, cx
                    ),
                }
            }
        }
        thread.end_running(cx);
    }
}

fn thread_fn(thread: CurrentThread) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
    Box::pin(new_thread(thread))
}

/// syscall handler entry
#[cfg(target_arch = "x86_64")]
async fn handle_syscall(thread: &CurrentThread, regs: &mut GeneralRegs) {
    trace!("syscall: {:#x?}", regs);
    let num = regs.rax as u32;
    let args = [regs.rdi, regs.rsi, regs.rdx, regs.r10, regs.r8, regs.r9];
    let mut syscall = Syscall {
        thread,
        #[cfg(feature = "std")]
        syscall_entry: kernel_hal_unix::syscall_entry as usize,
        #[cfg(not(feature = "std"))]
        syscall_entry: 0,
        thread_fn,
        regs,
    };
    regs.rax = syscall.syscall(num, args).await as usize;
}

#[cfg(target_arch = "riscv64")]
async fn handle_syscall(thread: &CurrentThread, cx: &mut UserContext) {
    trace!("syscall: {:#x?}", cx.general);
    let num = cx.general.a7 as u32;
    let args = [
        cx.general.a0,
        cx.general.a1,
        cx.general.a2,
        cx.general.a3,
        cx.general.a4,
        cx.general.a5,
    ];
    // add before fork
    cx.sepc += 4;

    //注意, 此时的regs没有原context所有权，故无法通过此regs修改寄存器
    //let regs = &mut (cx.general as GeneralRegs);

    let mut syscall = Syscall {
        thread,
        #[cfg(feature = "std")]
        syscall_entry: kernel_hal_unix::syscall_entry as usize,
        #[cfg(not(feature = "std"))]
        syscall_entry: 0,
        context: cx,
        thread_fn,
    };
    cx.general.a0 = syscall.syscall(num, args).await as usize;
}

// #[allow(dead_code)]
// fn super_mode_net_test() {
//     use smoltcp::socket::UdpPacketMetadata;
//     use smoltcp::socket::UdpSocket;
//     use smoltcp::socket::UdpSocketBuffer;
//     use smoltcp::time::Instant;
//     use core::str::from_utf8;
//     use kernel_hal::devices::get_net_driver;
//     use kernel_hal::timer_now;
//     use kernel_hal_bare::devices::net::e1000::E1000Interface;
//     use smoltcp::socket::SocketSet;

//     // udp s
//     let udp_rx_buffer = UdpSocketBuffer::new(vec![UdpPacketMetadata::EMPTY], vec![0; 64]);
//     let udp_tx_buffer = UdpSocketBuffer::new(vec![UdpPacketMetadata::EMPTY], vec![0; 128]);
//     // udp socket
//     let udp_socket = UdpSocket::new(udp_rx_buffer, udp_tx_buffer);

//     use alloc::vec;
//     // use alloc::vec::Vec;
//     let mut sockets = SocketSet::new(vec![]);
//     let udp_handle = sockets.add(udp_socket);

//     let e1000 = get_net_driver()[0].clone();
//         // let local = NET_DRIVERS.read()[0].clone();

//     if let Ok(_li) = e1000.downcast_arc::<E1000Interface>() {
//             // use std::os::unix::io::AsRawFd;
//             // let fd = li.iface.lock().device().as_raw_fd();
//             // let mut sockets = SOCKETS.lock();
//         loop {
//             // let _timestamp = Instant::now();
//             let timestamp = Instant::from_millis(timer_now().as_millis() as i64);
//             match _li.iface.lock().poll(&mut sockets, timestamp) {
//                 Ok(_a) => {
//                     // warn!("poll ok: {}", a);
//                 }
//                 Err(e) => {
//                     warn!("poll error: {}", e);
//                 }
//             }
//             {
//                 // udp bind
//                 let mut socket = sockets.get::<UdpSocket>(udp_handle);
//                 if !socket.is_open() {
//                     warn!("bind 6969");
//                     socket.bind(6969).unwrap()
//                 }
//                  // udp recv
//                 let client = match socket.recv() {
//                     Ok((data, endpoint)) => {
//                         warn!(" udp recv : {} form {}", from_utf8(data).unwrap(), endpoint);
//                         Some(endpoint)
//                     }
//                     Err(_) => None,
//                 };
//                 // udp send
//                 if let Some(endpoint) = client {
//                     let data = b"cargo test OK";
//                     warn!(
//                         "udp:6969 send data: {:?}",
//                         from_utf8(data.as_ref()).unwrap()
//                     );
//                     socket.send_slice(data, endpoint).unwrap();
//                 }
//             }
//         }
//     }
//     todo!();

// }
