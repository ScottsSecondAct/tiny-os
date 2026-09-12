// Peripheral driver instances for SPI, I2C, GPIO, PWM, and Serial.
//
// On RPi5, these use the RP1 southbridge drivers. On QEMU (no RP1),
// all operations return errors.

use arch::gpio::{GpioController, GpioError, PinMode, PullMode};
use arch::i2c::{I2cConfig, I2cDevice, I2cError};
use arch::pwm::{PwmConfig, PwmDevice, PwmError};
use arch::serial::{SerialConfig, SerialError, SerialPort};
use arch::spi::{SpiConfig, SpiDevice, SpiError};
use core::cell::UnsafeCell;

// --- SPI ---

struct SpiCell(UnsafeCell<SpiState>);
// SAFETY: Single-task access enforced by syscall serialization.
unsafe impl Sync for SpiCell {}

struct SpiState {
    configured: bool,
    #[cfg(feature = "bsp-rpi5")]
    driver: bsp::Rp1Spi,
}

static SPI: SpiCell = SpiCell(UnsafeCell::new(SpiState {
    configured: false,
    #[cfg(feature = "bsp-rpi5")]
    driver: bsp::Rp1Spi::new(),
}));

fn spi_state() -> &'static mut SpiState {
    // SAFETY: Single-task access via syscall serialization.
    unsafe { &mut *SPI.0.get() }
}

pub fn spi_configure(config: &SpiConfig) -> Result<(), SpiError> {
    let s = spi_state();
    #[cfg(feature = "bsp-rpi5")]
    {
        s.driver.configure(config)?;
        s.configured = true;
        Ok(())
    }
    #[cfg(not(feature = "bsp-rpi5"))]
    {
        let _ = (s, config);
        Err(SpiError::BusNotAvailable)
    }
}

pub fn spi_transfer(tx: &[u8], rx: &mut [u8]) -> Result<usize, SpiError> {
    let s = spi_state();
    if !s.configured {
        return Err(SpiError::BusNotAvailable);
    }
    #[cfg(feature = "bsp-rpi5")]
    {
        s.driver.transfer(tx, rx)
    }
    #[cfg(not(feature = "bsp-rpi5"))]
    {
        let _ = (tx, rx);
        Err(SpiError::BusNotAvailable)
    }
}

pub fn spi_write(data: &[u8]) -> Result<usize, SpiError> {
    let s = spi_state();
    if !s.configured {
        return Err(SpiError::BusNotAvailable);
    }
    #[cfg(feature = "bsp-rpi5")]
    {
        s.driver.write(data)
    }
    #[cfg(not(feature = "bsp-rpi5"))]
    {
        let _ = data;
        Err(SpiError::BusNotAvailable)
    }
}

pub fn spi_read(buf: &mut [u8]) -> Result<usize, SpiError> {
    let s = spi_state();
    if !s.configured {
        return Err(SpiError::BusNotAvailable);
    }
    #[cfg(feature = "bsp-rpi5")]
    {
        s.driver.read(buf)
    }
    #[cfg(not(feature = "bsp-rpi5"))]
    {
        let _ = buf;
        Err(SpiError::BusNotAvailable)
    }
}

// --- I2C ---

struct I2cCell(UnsafeCell<I2cState>);
unsafe impl Sync for I2cCell {}

struct I2cState {
    configured: bool,
    #[cfg(feature = "bsp-rpi5")]
    driver: bsp::Rp1I2c,
}

static I2C: I2cCell = I2cCell(UnsafeCell::new(I2cState {
    configured: false,
    #[cfg(feature = "bsp-rpi5")]
    driver: bsp::Rp1I2c::new(),
}));

fn i2c_state() -> &'static mut I2cState {
    unsafe { &mut *I2C.0.get() }
}

pub fn i2c_configure(config: &I2cConfig) -> Result<(), I2cError> {
    let s = i2c_state();
    #[cfg(feature = "bsp-rpi5")]
    {
        s.driver.configure(config)?;
        s.configured = true;
        Ok(())
    }
    #[cfg(not(feature = "bsp-rpi5"))]
    {
        let _ = (s, config);
        Err(I2cError::BusNotAvailable)
    }
}

pub fn i2c_write(addr: u8, data: &[u8]) -> Result<usize, I2cError> {
    let s = i2c_state();
    if !s.configured {
        return Err(I2cError::BusNotAvailable);
    }
    #[cfg(feature = "bsp-rpi5")]
    {
        s.driver.write(addr, data)
    }
    #[cfg(not(feature = "bsp-rpi5"))]
    {
        let _ = (addr, data);
        Err(I2cError::BusNotAvailable)
    }
}

pub fn i2c_read(addr: u8, buf: &mut [u8]) -> Result<usize, I2cError> {
    let s = i2c_state();
    if !s.configured {
        return Err(I2cError::BusNotAvailable);
    }
    #[cfg(feature = "bsp-rpi5")]
    {
        s.driver.read(addr, buf)
    }
    #[cfg(not(feature = "bsp-rpi5"))]
    {
        let _ = (addr, buf);
        Err(I2cError::BusNotAvailable)
    }
}

pub fn i2c_write_read(addr: u8, tx: &[u8], rx: &mut [u8]) -> Result<usize, I2cError> {
    let s = i2c_state();
    if !s.configured {
        return Err(I2cError::BusNotAvailable);
    }
    #[cfg(feature = "bsp-rpi5")]
    {
        s.driver.write_read(addr, tx, rx)
    }
    #[cfg(not(feature = "bsp-rpi5"))]
    {
        let _ = (addr, tx, rx);
        Err(I2cError::BusNotAvailable)
    }
}

// --- GPIO ---

struct GpioCell(UnsafeCell<GpioState>);
unsafe impl Sync for GpioCell {}

struct GpioState {
    #[cfg(feature = "bsp-rpi5")]
    driver: bsp::Rp1Gpio,
    #[cfg(not(feature = "bsp-rpi5"))]
    _phantom: (),
}

static GPIO: GpioCell = GpioCell(UnsafeCell::new(GpioState {
    #[cfg(feature = "bsp-rpi5")]
    driver: bsp::Rp1Gpio::new(),
    #[cfg(not(feature = "bsp-rpi5"))]
    _phantom: (),
}));

fn gpio_state() -> &'static mut GpioState {
    unsafe { &mut *GPIO.0.get() }
}

pub fn gpio_set_mode(pin: u8, mode: PinMode) -> Result<(), GpioError> {
    #[cfg(feature = "bsp-rpi5")]
    {
        gpio_state().driver.set_mode(pin, mode)
    }
    #[cfg(not(feature = "bsp-rpi5"))]
    {
        let _ = (pin, mode);
        Err(GpioError::NotConfigured)
    }
}

pub fn gpio_set_pull(pin: u8, pull: PullMode) -> Result<(), GpioError> {
    #[cfg(feature = "bsp-rpi5")]
    {
        gpio_state().driver.set_pull(pin, pull)
    }
    #[cfg(not(feature = "bsp-rpi5"))]
    {
        let _ = (pin, pull);
        Err(GpioError::NotConfigured)
    }
}

pub fn gpio_read(pin: u8) -> Result<bool, GpioError> {
    #[cfg(feature = "bsp-rpi5")]
    {
        gpio_state().driver.read(pin)
    }
    #[cfg(not(feature = "bsp-rpi5"))]
    {
        let _ = pin;
        Err(GpioError::NotConfigured)
    }
}

pub fn gpio_write(pin: u8, high: bool) -> Result<(), GpioError> {
    #[cfg(feature = "bsp-rpi5")]
    {
        gpio_state().driver.write(pin, high)
    }
    #[cfg(not(feature = "bsp-rpi5"))]
    {
        let _ = (pin, high);
        Err(GpioError::NotConfigured)
    }
}

// --- PWM ---

struct PwmCell(UnsafeCell<PwmState>);
unsafe impl Sync for PwmCell {}

struct PwmState {
    configured: bool,
    #[cfg(feature = "bsp-rpi5")]
    driver: bsp::Rp1Pwm,
}

static PWM: PwmCell = PwmCell(UnsafeCell::new(PwmState {
    configured: false,
    #[cfg(feature = "bsp-rpi5")]
    driver: bsp::Rp1Pwm::new(),
}));

fn pwm_state() -> &'static mut PwmState {
    // SAFETY: Single-task access via syscall serialization.
    unsafe { &mut *PWM.0.get() }
}

pub fn pwm_configure(config: &PwmConfig) -> Result<(), PwmError> {
    let s = pwm_state();
    #[cfg(feature = "bsp-rpi5")]
    {
        s.driver.configure(config)?;
        s.configured = true;
        Ok(())
    }
    #[cfg(not(feature = "bsp-rpi5"))]
    {
        let _ = (s, config);
        Err(PwmError::HardwareNotAvailable)
    }
}

pub fn pwm_set_duty(channel: u8, duty: u8) -> Result<(), PwmError> {
    let s = pwm_state();
    if !s.configured {
        return Err(PwmError::NotEnabled);
    }
    #[cfg(feature = "bsp-rpi5")]
    {
        s.driver.set_duty(channel, duty)
    }
    #[cfg(not(feature = "bsp-rpi5"))]
    {
        let _ = (channel, duty);
        Err(PwmError::HardwareNotAvailable)
    }
}

pub fn pwm_enable(channel: u8) -> Result<(), PwmError> {
    let s = pwm_state();
    if !s.configured {
        return Err(PwmError::NotEnabled);
    }
    #[cfg(feature = "bsp-rpi5")]
    {
        s.driver.enable(channel)
    }
    #[cfg(not(feature = "bsp-rpi5"))]
    {
        let _ = channel;
        Err(PwmError::HardwareNotAvailable)
    }
}

pub fn pwm_disable(channel: u8) -> Result<(), PwmError> {
    let s = pwm_state();
    if !s.configured {
        return Err(PwmError::NotEnabled);
    }
    #[cfg(feature = "bsp-rpi5")]
    {
        s.driver.disable(channel)
    }
    #[cfg(not(feature = "bsp-rpi5"))]
    {
        let _ = channel;
        Err(PwmError::HardwareNotAvailable)
    }
}

// --- Serial ---

struct SerialCell(UnsafeCell<SerialState>);
unsafe impl Sync for SerialCell {}

struct SerialState {
    #[cfg(feature = "bsp-rpi5")]
    driver: bsp::Rp1Serial,
    #[cfg(not(feature = "bsp-rpi5"))]
    _phantom: (),
}

static SERIAL: SerialCell = SerialCell(UnsafeCell::new(SerialState {
    #[cfg(feature = "bsp-rpi5")]
    driver: bsp::Rp1Serial::new(),
    #[cfg(not(feature = "bsp-rpi5"))]
    _phantom: (),
}));

fn serial_state() -> &'static mut SerialState {
    // SAFETY: Single-task access via syscall serialization.
    unsafe { &mut *SERIAL.0.get() }
}

pub fn serial_open(config: &SerialConfig) -> Result<(), SerialError> {
    #[cfg(feature = "bsp-rpi5")]
    {
        serial_state().driver.open(config)
    }
    #[cfg(not(feature = "bsp-rpi5"))]
    {
        let _ = config;
        Err(SerialError::PortNotAvailable)
    }
}

pub fn serial_write(port: u8, data: &[u8]) -> Result<usize, SerialError> {
    #[cfg(feature = "bsp-rpi5")]
    {
        serial_state().driver.write(port, data)
    }
    #[cfg(not(feature = "bsp-rpi5"))]
    {
        let _ = (port, data);
        Err(SerialError::PortNotAvailable)
    }
}

pub fn serial_read(port: u8, buf: &mut [u8]) -> Result<usize, SerialError> {
    #[cfg(feature = "bsp-rpi5")]
    {
        serial_state().driver.read(port, buf)
    }
    #[cfg(not(feature = "bsp-rpi5"))]
    {
        let _ = (port, buf);
        Err(SerialError::PortNotAvailable)
    }
}

pub fn serial_close(port: u8) -> Result<(), SerialError> {
    #[cfg(feature = "bsp-rpi5")]
    {
        serial_state().driver.close(port)
    }
    #[cfg(not(feature = "bsp-rpi5"))]
    {
        let _ = port;
        Err(SerialError::PortNotAvailable)
    }
}
