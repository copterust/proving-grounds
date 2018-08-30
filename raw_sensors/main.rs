#![deny(warnings)]
#![no_std]
#![no_main]

#[allow(unused)]
use panic_abort;

use core::fmt::{self, Write};

use hal::prelude::*;
use hal::time::Bps;
use hal::{delay, serial};
use nb;
use rt::{entry, exception};
use stm32f30x::interrupt;

use mpu9250::Mpu9250;

static mut L: Option<Logger<hal::serial::Tx<hal::stm32f30x::USART1>>> = None;
static mut RX: Option<hal::serial::Rx<hal::stm32f30x::USART1>> = None;
static mut QUIET: bool = true;
const TURN_QUIET: u8 = 'q' as u8;

entry!(main);
fn main() -> ! {
    let device = hal::stm32f30x::Peripherals::take().unwrap();
    let core = cortex_m::Peripherals::take().unwrap();
    let mut rcc = device.RCC.constrain();
    let mut flash = device.FLASH.constrain();
    let clocks = rcc
        .cfgr
        .sysclk(64.mhz())
        .pclk1(32.mhz())
        .pclk2(32.mhz())
        .freeze(&mut flash.acr);
    let gpioa = device.GPIOA.split(&mut rcc.ahb);
    let gpiob = device.GPIOB.split(&mut rcc.ahb);
    let mut serial = device
        .USART1
        .serial((gpioa.pa9, gpioa.pa10), Bps(115200), clocks);
    let ser_int = serial.get_interrupt();
    serial.listen(serial::Event::Rxne);
    let (mut tx, rx) = serial.split();
    // COBS frame
    tx.write(0x00).unwrap();
    unsafe {
        L = Some(Logger { tx });
        RX = Some(rx);
    };
    let l = unsafe { extract(&mut L) };
    write!(l, "logger ok\r\n");
    let mut delay = delay::Delay::new(core.SYST, clocks);
    // SPI1
    let ncs = gpiob.pb9.output().push_pull();
    let spi = device.SPI1.spi(
        // scl_sck, ad0_sd0_miso, sda_sdi_mosi,
        (gpiob.pb3, gpiob.pb4, gpiob.pb5),
        mpu9250::MODE,
        1.mhz(),
        clocks,
    );
    write!(l, "spi ok\r\n");
    let mut mpu = match Mpu9250::imu_default(spi, ncs, &mut delay) {
        Ok(m) => m,
        Err(e) => {
            write!(l, "Mpu init error: {:?}", e);
            panic!("mpu err");
        }
    };
    write!(l, "mpu ok\r\n");
    let accel_biases = match mpu.calibrate_at_rest(&mut delay) {
        Ok(ab) => ab,
        Err(e) => {
            write!(l, "Mpu calib error: {:?}", e);
            panic!("mpu err");
        }
    };
    write!(l, "calibration ok: {:?}\r\n", accel_biases);
    unsafe { cortex_m::interrupt::enable() };
    let mut nvic = core.NVIC;
    nvic.enable(ser_int);

    let tmr = hal::time::MonoTimer::new(core.DWT, clocks);
    let now = tmr.now();
    write!(
        l,
        "All ok; starting: {:?}; freq: {:?}; Press 'q' to toggle verbosity!\r\n",
        now.elapsed(),
        tmr.frequency()
    );
    loop {
        let t = now.elapsed();
        match mpu.all() {
            Ok(meas) => {
                let gyro = meas.gyro;
                let accel = meas.accel - accel_biases;
                if unsafe { !QUIET } {
                    write!(
                        l,
                        "IMU: {:?}; g({};{};{}); a({};{};{})\r\n",
                        t, gyro.x, gyro.y, gyro.z, accel.x, accel.y, accel.z
                    );
                }
            }
            Err(e) => {
                write!(l, "Err: {:?}; {:?}", t, e);
            }
        }
    }
}

unsafe fn extract<T>(opt: &'static mut Option<T>) -> &'static mut T {
    match opt {
        Some(ref mut x) => &mut *x,
        None => panic!("extract"),
    }
}

struct Logger<W: ehal::serial::Write<u8>> {
    tx: W,
}
impl<W: ehal::serial::Write<u8>> fmt::Write for Logger<W> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            match self.write_char(c) {
                Ok(_) => {}
                Err(_) => {}
            }
        }
        match self.tx.flush() {
            Ok(_) => {}
            Err(_) => {}
        };

        Ok(())
    }

    fn write_char(&mut self, s: char) -> fmt::Result {
        match nb::block!(self.tx.write(s as u8)) {
            Ok(_) => {}
            Err(_) => {}
        }
        Ok(())
    }
}

interrupt!(USART1_EXTI25, usart_exti25);
fn usart_exti25() {
    let rx = unsafe { extract(&mut RX) };
    let l = unsafe { extract(&mut L) };
    match rx.read() {
        Ok(b) => {
            if b == TURN_QUIET {
                unsafe {
                    QUIET = !QUIET;
                }
            } else {
                // echo byte as is
                write!(l, "{}", b as char);
            }
        }
        Err(nb::Error::WouldBlock) => {}
        Err(nb::Error::Other(e)) => match e {
            serial::Error::Overrun => {
                rx.clear_overrun_error();
            }
            serial::Error::Framing => {
                rx.clear_framing_error();
            }
            serial::Error::Noise => {
                rx.clear_noise_error();
            }
            _ => {
                write!(l, "read error: {:?}", e);
            }
        },
    };
}

exception!(HardFault, |ef| {
    let l = extract(&mut L);
    write!(l, "hard fault at {:?}", ef);
    panic!("HardFault at {:#?}", ef);
});

exception!(*, |irqn| {
    let l = extract(&mut L);
    write!(l, "Interrupt: {}", irqn);
    panic!("Unhandled exception (IRQn = {})", irqn);
});
