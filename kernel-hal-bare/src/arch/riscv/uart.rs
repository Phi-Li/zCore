use super::consts::UART_BASE;
use crate::{phys_to_virt, putfmt};
use core::convert::TryInto;
use core::fmt::{Error, Write};

pub struct Uart {
    base_address: usize,
}

// 结构体Uart的实现块
impl Uart {
    pub fn new(base_address: usize) -> Self {
        Uart { base_address }
    }

    #[cfg(not(feature = "board_d1"))]
    pub fn simple_init(&mut self) {
        let ptr = self.base_address as *mut u8;
        unsafe {
            // Enable FIFO; (base + 2)
            ptr.add(2).write_volatile(0xC7);

            // MODEM Ctrl; (base + 4)
            ptr.add(4).write_volatile(0x0B);

            // Enable interrupts; (base + 1)
            ptr.add(1).write_volatile(0x01);
        }
    }

    #[cfg(feature = "board_d1")]
    pub fn simple_init(&mut self) {
        let ptr = self.base_address as *mut u32;
        unsafe {
            // Enable FIFO; (base + 2)
            ptr.add(2).write_volatile(0x7);

            // MODEM Ctrl; (base + 4)
            ptr.add(4).write_volatile(0x3);

            //D1 ALLWINNER的uart中断使能
            // D1 UART_IER offset = 0x4
            //
            // Enable interrupts; (base + 1)
            ptr.add(1).write_volatile(0x1);
        }
    }

    pub fn get(&mut self) -> Option<u8> {
        #[cfg(not(feature = "board_d1"))]
        let ptr = self.base_address as *mut u8;
        #[cfg(feature = "board_d1")]
        let ptr = self.base_address as *mut u32;

        unsafe {
            //查看LSR的DR位为1则有数据
            if ptr.add(5).read_volatile() & 0b1 == 0 {
                None
            } else {
                Some((ptr.add(0).read_volatile() & 0xff) as u8)
            }
        }
    }

    pub fn put(&mut self, c: u8) {
        let ptr = self.base_address as *mut u8;
        unsafe {
            //此时transmitter empty
            ptr.add(0).write_volatile(c);
        }
    }
}

// 需要实现的write_str()重要函数
impl Write for Uart {
    fn write_str(&mut self, out: &str) -> Result<(), Error> {
        for c in out.bytes() {
            self.put(c);
        }
        Ok(())
    }
}

pub fn handle_interrupt() {
    let mut my_uart = Uart::new(phys_to_virt(UART_BASE));
    if let Some(c) = my_uart.get() {
        let c = c & 0xff;
        //CONSOLE
        super::serial_put(c);

        /*
         * 因serial_write()已可以被回调输出了，这里则不再需要了
        match c {
            0x7f => { //0x8 [backspace] ; 而实际qemu运行，[backspace]键输出0x7f, 表示del
                bare_print!("{} {}", 8 as char, 8 as char);
            },
            10 | 13 => { // 新行或回车
                bare_println!();
            },
            _ => {
                bare_print!("{}", c as char);
            },
        }
        */
    }
}
