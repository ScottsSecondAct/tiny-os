// SDHCI (SD Host Controller Interface) driver for BCM2711/BCM2712 EMMC2.
//
// Implements the standard SDHCI register layout at the EMMC2 base address.
// Supports PIO (Programmed I/O) mode for data transfer.

use core::cell::UnsafeCell;
use crate::block::{BlockDevice, BlockError};

// SDHCI register offsets (all 32-bit aligned).
const _SDHCI_DMA_ADDRESS: usize = 0x00;
const SDHCI_BLOCK_SIZE_COUNT: usize = 0x04;
const SDHCI_ARGUMENT: usize = 0x08;
const SDHCI_XFER_MODE_CMD: usize = 0x0C;
const SDHCI_RESPONSE0: usize = 0x10;
const SDHCI_RESPONSE1: usize = 0x14;
const SDHCI_RESPONSE2: usize = 0x18;
const SDHCI_RESPONSE3: usize = 0x1C;
const SDHCI_BUFFER_DATA: usize = 0x20;
const SDHCI_PRESENT_STATE: usize = 0x24;
const SDHCI_HOST_CONTROL: usize = 0x28;
const SDHCI_CLOCK_CONTROL: usize = 0x2C;
const SDHCI_INT_STATUS: usize = 0x30;
const SDHCI_INT_ENABLE: usize = 0x34;
const SDHCI_SIGNAL_ENABLE: usize = 0x38;
const SDHCI_CAPABILITIES: usize = 0x40;
const SDHCI_SLOTISR_VER: usize = 0xFC;

// Present state bits.
const STATE_CMD_INHIBIT: u32 = 1 << 0;
const STATE_DAT_INHIBIT: u32 = 1 << 1;
const _STATE_BUF_WRITE_EN: u32 = 1 << 10;
const _STATE_BUF_READ_EN: u32 = 1 << 11;
const _STATE_CARD_INSERTED: u32 = 1 << 16;

// Normal interrupt status bits.
const INT_CMD_COMPLETE: u32 = 1 << 0;
const INT_XFER_COMPLETE: u32 = 1 << 1;
const INT_BUF_WRITE_READY: u32 = 1 << 4;
const INT_BUF_READ_READY: u32 = 1 << 5;
const INT_ERROR: u32 = 1 << 15;
const INT_ERR_CMD_TIMEOUT: u32 = 1 << 16;
const INT_ERR_CMD_CRC: u32 = 1 << 17;
const INT_ERR_DATA_TIMEOUT: u32 = 1 << 20;
const _INT_ERR_DATA_CRC: u32 = 1 << 21;
const _INT_ALL_NORMAL: u32 = 0x0000_FFFF;
const _INT_ALL_ERROR: u32 = 0xFFFF_0000;

// Clock control bits.
const CLK_INTERNAL_EN: u32 = 1 << 0;
const CLK_STABLE: u32 = 1 << 1;
const CLK_SD_EN: u32 = 1 << 2;

// Software reset bits (in clock control register upper byte).
const RESET_ALL: u32 = 1 << 24;
const _RESET_CMD: u32 = 1 << 25;
const _RESET_DATA: u32 = 1 << 26;

// Power control bits.
const POWER_ON: u32 = 1 << 8;
const POWER_3V3: u32 = 7 << 9;

// Command flags for SDHCI_XFER_MODE_CMD register.
const CMD_RESP_NONE: u32 = 0 << 16;
const CMD_RESP_136: u32 = 1 << 16;
const CMD_RESP_48: u32 = 2 << 16;
const CMD_RESP_48_BUSY: u32 = 3 << 16;
const CMD_CRC_CHECK: u32 = 1 << 19;
const CMD_INDEX_CHECK: u32 = 1 << 20;
const CMD_DATA_PRESENT: u32 = 1 << 21;
const CMD_INDEX_SHIFT: u32 = 24;

// Transfer mode bits.
const XFER_READ: u32 = 1 << 4;
const XFER_BLOCK_COUNT_EN: u32 = 1 << 1;

// SD card commands.
const SD_CMD_GO_IDLE: u32 = 0;
const SD_CMD_SEND_IF_COND: u32 = 8;
const SD_CMD_SEND_CSD: u32 = 9;
const _SD_CMD_STOP_TRANSMISSION: u32 = 12;
const SD_CMD_SET_BLOCKLEN: u32 = 16;
const SD_CMD_READ_SINGLE: u32 = 17;
const _SD_CMD_READ_MULTI: u32 = 18;
const SD_CMD_WRITE_SINGLE: u32 = 24;
const _SD_CMD_WRITE_MULTI: u32 = 25;
const SD_CMD_APP_CMD: u32 = 55;
const SD_ACMD_SEND_OP_COND: u32 = 41;
const SD_CMD_ALL_SEND_CID: u32 = 2;
const SD_CMD_SEND_RELATIVE_ADDR: u32 = 3;
const SD_CMD_SELECT_CARD: u32 = 7;

// Host Control 2 register (UHS-I mode).
const SDHCI_HOST_CONTROL2: usize = 0x3E;
const HC2_1V8_SIGNALING: u32 = 1 << 3;
const HC2_UHS_SDR104: u32 = 3;
const HC2_EXEC_TUNING: u32 = 1 << 6;
const HC2_TUNING_COMPLETE: u32 = 1 << 7;

// ADMA2 system address register.
const SDHCI_ADMA_ADDR: usize = 0x58;

// ADMA2 descriptor attributes.
const ADMA2_VALID: u32 = 1 << 0;
const ADMA2_END: u32 = 1 << 1;
const ADMA2_ACT_TRAN: u32 = 2 << 4;

// Transfer mode bit for ADMA2 (DMA enable in host control).
const HOST_CTRL_DMA_SEL_ADMA2: u32 = 2 << 3;

// SD tuning command.
const SD_CMD_SEND_TUNING: u32 = 19;

const BLOCK_SIZE: usize = 512;
const PIO_TIMEOUT: u64 = 1_000_000;

#[repr(C, align(8))]
struct Adma2Desc {
    attr_len: u32,
    addr: u32,
}

static mut ADMA2_TABLE: [Adma2Desc; 4] = [
    Adma2Desc { attr_len: 0, addr: 0 },
    Adma2Desc { attr_len: 0, addr: 0 },
    Adma2Desc { attr_len: 0, addr: 0 },
    Adma2Desc { attr_len: 0, addr: 0 },
];

pub struct Emmc2 {
    base: usize,
    rca: u32,
    sdhc: bool,
    block_count: u64,
    initialized: bool,
    use_adma2: bool,
}

struct Emmc2Cell(UnsafeCell<Option<Emmc2>>);
// SAFETY: Access is serialized — init is single-core before interrupts,
// subsequent access is spinlock-protected by the kernel storage layer.
unsafe impl Sync for Emmc2Cell {}

static EMMC: Emmc2Cell = Emmc2Cell(UnsafeCell::new(None));

impl Emmc2 {
    pub const fn new(base: usize) -> Self {
        Self {
            base,
            rca: 0,
            sdhc: false,
            block_count: 0,
            initialized: false,
            use_adma2: false,
        }
    }

    #[inline(always)]
    fn read(&self, offset: usize) -> u32 {
        // SAFETY: base is a valid SDHCI MMIO address set by the BSP.
        unsafe { core::ptr::read_volatile((self.base + offset) as *const u32) }
    }

    #[inline(always)]
    fn write(&self, offset: usize, val: u32) {
        // SAFETY: Same as read.
        unsafe { core::ptr::write_volatile((self.base + offset) as *mut u32, val) }
    }

    fn delay_us(&self, us: u64) {
        let freq: u64;
        unsafe { core::arch::asm!("mrs {}, cntfrq_el0", out(reg) freq) };
        let ticks = freq * us / 1_000_000;
        let start: u64;
        unsafe { core::arch::asm!("mrs {}, cntvct_el0", out(reg) start) };
        loop {
            let now: u64;
            unsafe { core::arch::asm!("mrs {}, cntvct_el0", out(reg) now) };
            if now - start >= ticks {
                break;
            }
            core::hint::spin_loop();
        }
    }

    fn wait_cmd_ready(&self) -> Result<(), BlockError> {
        for _ in 0..PIO_TIMEOUT {
            if self.read(SDHCI_PRESENT_STATE) & STATE_CMD_INHIBIT == 0 {
                return Ok(());
            }
            core::hint::spin_loop();
        }
        Err(BlockError::Timeout)
    }

    fn wait_dat_ready(&self) -> Result<(), BlockError> {
        for _ in 0..PIO_TIMEOUT {
            if self.read(SDHCI_PRESENT_STATE) & STATE_DAT_INHIBIT == 0 {
                return Ok(());
            }
            core::hint::spin_loop();
        }
        Err(BlockError::Timeout)
    }

    fn send_command(&self, cmd_idx: u32, arg: u32, flags: u32) -> Result<u32, BlockError> {
        self.wait_cmd_ready()?;

        // Clear all interrupt status bits.
        self.write(SDHCI_INT_STATUS, 0xFFFF_FFFF);

        self.write(SDHCI_ARGUMENT, arg);

        let cmd_val = (cmd_idx << CMD_INDEX_SHIFT) | flags;
        self.write(SDHCI_XFER_MODE_CMD, cmd_val);

        // Wait for command complete or error.
        for _ in 0..PIO_TIMEOUT {
            let status = self.read(SDHCI_INT_STATUS);
            if status & INT_ERROR != 0 {
                self.write(SDHCI_INT_STATUS, status);
                if status & INT_ERR_CMD_TIMEOUT != 0 {
                    return Err(BlockError::Timeout);
                }
                if status & INT_ERR_CMD_CRC != 0 {
                    return Err(BlockError::Crc);
                }
                return Err(BlockError::IoError);
            }
            if status & INT_CMD_COMPLETE != 0 {
                self.write(SDHCI_INT_STATUS, INT_CMD_COMPLETE);
                return Ok(self.read(SDHCI_RESPONSE0));
            }
            core::hint::spin_loop();
        }
        Err(BlockError::Timeout)
    }

    fn send_app_command(&self, acmd_idx: u32, arg: u32, flags: u32) -> Result<u32, BlockError> {
        self.send_command(
            SD_CMD_APP_CMD,
            self.rca << 16,
            CMD_RESP_48 | CMD_CRC_CHECK | CMD_INDEX_CHECK,
        )?;
        self.send_command(acmd_idx, arg, flags)
    }

    fn controller_reset(&self) -> Result<(), BlockError> {
        self.write(SDHCI_CLOCK_CONTROL, RESET_ALL);
        for _ in 0..PIO_TIMEOUT {
            if self.read(SDHCI_CLOCK_CONTROL) & RESET_ALL == 0 {
                return Ok(());
            }
            core::hint::spin_loop();
        }
        Err(BlockError::Timeout)
    }

    fn set_clock(&self, freq_khz: u32) -> Result<(), BlockError> {
        // Disable SD clock.
        let mut ctrl = self.read(SDHCI_CLOCK_CONTROL);
        ctrl &= !CLK_SD_EN;
        self.write(SDHCI_CLOCK_CONTROL, ctrl & 0x00FF_FFFF);

        // Read base clock from capabilities.
        let caps = self.read(SDHCI_CAPABILITIES);
        let base_mhz = (caps >> 8) & 0xFF;
        if base_mhz == 0 {
            return Ok(());
        }

        let base_khz = base_mhz * 1000;
        let div = if freq_khz >= base_khz {
            0
        } else {
            let mut d = 1u32;
            while d < 2046 && base_khz / (2 * d) > freq_khz {
                d += 1;
            }
            d
        };

        let div_lo = (div & 0xFF) << 8;
        let div_hi = ((div >> 8) & 0x03) << 6;
        let clk_val = div_lo | div_hi | CLK_INTERNAL_EN;
        self.write(SDHCI_CLOCK_CONTROL, clk_val);

        // Wait for internal clock stable.
        for _ in 0..PIO_TIMEOUT {
            if self.read(SDHCI_CLOCK_CONTROL) & CLK_STABLE != 0 {
                break;
            }
            core::hint::spin_loop();
        }

        // Enable SD clock.
        let clk_val = self.read(SDHCI_CLOCK_CONTROL) | CLK_SD_EN;
        self.write(SDHCI_CLOCK_CONTROL, clk_val);

        self.delay_us(1000);
        Ok(())
    }

    fn set_power(&self) {
        let mut ctrl = self.read(SDHCI_HOST_CONTROL);
        ctrl &= 0xFFFF_00FF;
        ctrl |= POWER_ON | POWER_3V3;
        self.write(SDHCI_HOST_CONTROL, ctrl);
    }

    pub fn controller_init(&mut self) -> Result<(), BlockError> {
        self.controller_reset()?;

        self.set_power();
        self.delay_us(10_000);

        // Enable all interrupt status bits.
        self.write(SDHCI_INT_ENABLE, 0xFFFF_FFFF);
        self.write(SDHCI_SIGNAL_ENABLE, 0);

        // Set timeout to maximum.
        let ctrl = self.read(SDHCI_CLOCK_CONTROL);
        self.write(SDHCI_CLOCK_CONTROL, (ctrl & 0xFF00_FFFF) | (0x0E << 16));

        self.set_clock(400)?;
        Ok(())
    }

    pub fn card_init(&mut self) -> Result<(), BlockError> {
        self.delay_us(10_000);

        // CMD0: GO_IDLE_STATE
        let _ = self.send_command(SD_CMD_GO_IDLE, 0, CMD_RESP_NONE);
        self.delay_us(5000);

        // CMD8: SEND_IF_COND (SD v2 check)
        let _sd_v2 = match self.send_command(
            SD_CMD_SEND_IF_COND,
            0x000001AA,
            CMD_RESP_48 | CMD_CRC_CHECK | CMD_INDEX_CHECK,
        ) {
            Ok(resp) => {
                if resp & 0x1FF != 0x1AA {
                    return Err(BlockError::IoError);
                }
                true
            }
            Err(BlockError::Timeout) => false,
            Err(e) => return Err(e),
        };

        // ACMD41: SD_SEND_OP_COND — poll until card ready.
        let mut ocr = 0u32;
        let acmd41_arg = if _sd_v2 { 0x40FF_8000 } else { 0x00FF_8000 };
        for _ in 0..100 {
            match self.send_app_command(
                SD_ACMD_SEND_OP_COND,
                acmd41_arg,
                CMD_RESP_48,
            ) {
                Ok(resp) => {
                    ocr = resp;
                    if ocr & (1 << 31) != 0 {
                        break;
                    }
                }
                Err(BlockError::Timeout) => {}
                Err(e) => return Err(e),
            }
            self.delay_us(10_000);
        }

        if ocr & (1 << 31) == 0 {
            return Err(BlockError::Timeout);
        }

        self.sdhc = ocr & (1 << 30) != 0;

        // CMD2: ALL_SEND_CID
        self.send_command(SD_CMD_ALL_SEND_CID, 0, CMD_RESP_136 | CMD_CRC_CHECK)?;

        // CMD3: SEND_RELATIVE_ADDR
        let resp = self.send_command(
            SD_CMD_SEND_RELATIVE_ADDR,
            0,
            CMD_RESP_48 | CMD_CRC_CHECK | CMD_INDEX_CHECK,
        )?;
        self.rca = resp >> 16;

        // CMD9: SEND_CSD — get card parameters.
        self.send_command(
            SD_CMD_SEND_CSD,
            self.rca << 16,
            CMD_RESP_136 | CMD_CRC_CHECK,
        )?;

        // Parse card size from CSD response registers.
        self.block_count = self.parse_card_size();

        // CMD7: SELECT_CARD
        self.send_command(
            SD_CMD_SELECT_CARD,
            self.rca << 16,
            CMD_RESP_48_BUSY | CMD_CRC_CHECK | CMD_INDEX_CHECK,
        )?;

        // Set block length to 512 for SDSC cards.
        if !self.sdhc {
            self.send_command(
                SD_CMD_SET_BLOCKLEN,
                BLOCK_SIZE as u32,
                CMD_RESP_48 | CMD_CRC_CHECK | CMD_INDEX_CHECK,
            )?;
        }

        // Increase clock to 25 MHz for data transfer.
        self.set_clock(25000)?;

        // Attempt SDR104 UHS-I mode (208 MHz, 1.8V signaling, tuning).
        if self.sdhc {
            match self.try_sdr104() {
                Ok(()) => self.use_adma2 = true,
                Err(_) => { /* stay at 25 MHz PIO */ }
            }
        }

        self.initialized = true;
        Ok(())
    }

    fn try_sdr104(&mut self) -> Result<(), BlockError> {
        // Switch to 1.8V signaling.
        let hc2 = self.read(SDHCI_HOST_CONTROL2 as usize) as u32;
        self.write(
            SDHCI_HOST_CONTROL2 as usize,
            (hc2 & !0x7) | HC2_UHS_SDR104 | HC2_1V8_SIGNALING,
        );
        self.delay_us(5000);

        // Enable ADMA2 in host control register.
        let hc1 = self.read(SDHCI_HOST_CONTROL);
        self.write(SDHCI_HOST_CONTROL, (hc1 & !(0x3 << 3)) | HOST_CTRL_DMA_SEL_ADMA2);

        // Set clock to 208 MHz.
        self.set_clock(208000)?;

        // Execute tuning (CMD19).
        let hc2 = self.read(SDHCI_HOST_CONTROL2 as usize);
        self.write(SDHCI_HOST_CONTROL2 as usize, hc2 | HC2_EXEC_TUNING);

        for _ in 0..40 {
            let _ = self.send_command(
                SD_CMD_SEND_TUNING,
                0,
                CMD_RESP_48 | CMD_DATA_PRESENT | XFER_READ,
            );
            self.delay_us(1000);

            let hc2 = self.read(SDHCI_HOST_CONTROL2 as usize);
            if hc2 & HC2_EXEC_TUNING == 0 {
                if hc2 & HC2_TUNING_COMPLETE != 0 {
                    return Ok(());
                }
                break;
            }
        }

        // Tuning failed — revert to 25 MHz.
        let hc2 = self.read(SDHCI_HOST_CONTROL2 as usize);
        self.write(SDHCI_HOST_CONTROL2 as usize, hc2 & !(HC2_1V8_SIGNALING | 0x7));
        let hc1 = self.read(SDHCI_HOST_CONTROL);
        self.write(SDHCI_HOST_CONTROL, hc1 & !(0x3 << 3));
        self.set_clock(25000)?;
        Err(BlockError::Timeout)
    }

    fn read_block_adma2(&self, lba: u64, buf: &mut [u8]) -> Result<(), BlockError> {
        self.wait_cmd_ready()?;
        self.wait_dat_ready()?;

        // SAFETY: ADMA2_TABLE is only accessed during serialized block I/O.
        unsafe {
            ADMA2_TABLE[0].attr_len = (BLOCK_SIZE as u32) << 16
                | ADMA2_VALID | ADMA2_END | ADMA2_ACT_TRAN;
            ADMA2_TABLE[0].addr = buf.as_ptr() as u32;

            core::arch::asm!("dsb sy");

            self.write(SDHCI_ADMA_ADDR, &raw const ADMA2_TABLE as usize as u32);
        }

        self.write(SDHCI_BLOCK_SIZE_COUNT, (1 << 16) | BLOCK_SIZE as u32);

        let addr = if self.sdhc { lba as u32 } else { (lba * BLOCK_SIZE as u64) as u32 };
        self.write(SDHCI_INT_STATUS, 0xFFFF_FFFF);
        self.write(SDHCI_ARGUMENT, addr);

        let cmd_val = (SD_CMD_READ_SINGLE << CMD_INDEX_SHIFT)
            | CMD_RESP_48 | CMD_CRC_CHECK | CMD_INDEX_CHECK
            | CMD_DATA_PRESENT | XFER_READ | XFER_BLOCK_COUNT_EN;
        self.write(SDHCI_XFER_MODE_CMD, cmd_val);

        for _ in 0..PIO_TIMEOUT {
            let status = self.read(SDHCI_INT_STATUS);
            if status & INT_ERROR != 0 {
                self.write(SDHCI_INT_STATUS, status);
                return Err(BlockError::IoError);
            }
            if status & INT_XFER_COMPLETE != 0 {
                self.write(SDHCI_INT_STATUS, INT_XFER_COMPLETE);
                unsafe { core::arch::asm!("dsb sy"); }
                return Ok(());
            }
            core::hint::spin_loop();
        }
        Err(BlockError::Timeout)
    }

    fn write_block_adma2(&self, lba: u64, buf: &[u8]) -> Result<(), BlockError> {
        self.wait_cmd_ready()?;
        self.wait_dat_ready()?;

        // SAFETY: Same as read_block_adma2.
        unsafe {
            ADMA2_TABLE[0].attr_len = (BLOCK_SIZE as u32) << 16
                | ADMA2_VALID | ADMA2_END | ADMA2_ACT_TRAN;
            ADMA2_TABLE[0].addr = buf.as_ptr() as u32;

            core::arch::asm!("dsb sy");

            self.write(SDHCI_ADMA_ADDR, &raw const ADMA2_TABLE as usize as u32);
        }

        self.write(SDHCI_BLOCK_SIZE_COUNT, (1 << 16) | BLOCK_SIZE as u32);

        let addr = if self.sdhc { lba as u32 } else { (lba * BLOCK_SIZE as u64) as u32 };
        self.write(SDHCI_INT_STATUS, 0xFFFF_FFFF);
        self.write(SDHCI_ARGUMENT, addr);

        let cmd_val = (SD_CMD_WRITE_SINGLE << CMD_INDEX_SHIFT)
            | CMD_RESP_48 | CMD_CRC_CHECK | CMD_INDEX_CHECK
            | CMD_DATA_PRESENT | XFER_BLOCK_COUNT_EN;
        self.write(SDHCI_XFER_MODE_CMD, cmd_val);

        for _ in 0..PIO_TIMEOUT {
            let status = self.read(SDHCI_INT_STATUS);
            if status & INT_ERROR != 0 {
                self.write(SDHCI_INT_STATUS, status);
                return Err(BlockError::IoError);
            }
            if status & INT_XFER_COMPLETE != 0 {
                self.write(SDHCI_INT_STATUS, INT_XFER_COMPLETE);
                return Ok(());
            }
            core::hint::spin_loop();
        }
        Err(BlockError::Timeout)
    }

    fn parse_card_size(&self) -> u64 {
        let _resp0 = self.read(SDHCI_RESPONSE0) as u64;
        let resp1 = self.read(SDHCI_RESPONSE1) as u64;
        let resp2 = self.read(SDHCI_RESPONSE2) as u64;
        let resp3 = self.read(SDHCI_RESPONSE3) as u64;

        // CSD response is in RESP[3:0], shifted by 8 bits in SDHCI.
        // CSD v2 (SDHC/SDXC): C_SIZE in bits [69:48]
        // CSD v1 (SDSC): C_SIZE in bits [73:62], C_SIZE_MULT in bits [49:47]

        let csd_structure = ((resp3 >> 22) & 0x3) as u8;

        if csd_structure == 1 {
            // CSD v2 (SDHC/SDXC)
            let c_size = ((resp1 >> 8) & 0x3F_FFFF) as u64;
            (c_size + 1) * 1024
        } else {
            // CSD v1 (SDSC)
            let c_size = (((resp2 & 0x3) << 10) | ((resp1 >> 22) & 0x3FF)) as u64;
            let c_size_mult = ((resp1 >> 7) & 0x7) as u32;
            let read_bl_len = ((resp2 >> 8) & 0xF) as u32;
            let mult = 1u64 << (c_size_mult + 2);
            let block_len = 1u64 << read_bl_len;
            (c_size + 1) * mult * block_len / BLOCK_SIZE as u64
        }
    }

    fn read_block_pio(&self, lba: u64, buf: &mut [u8]) -> Result<(), BlockError> {
        self.wait_cmd_ready()?;
        self.wait_dat_ready()?;

        // Set block size and count.
        self.write(SDHCI_BLOCK_SIZE_COUNT, (1 << 16) | BLOCK_SIZE as u32);

        let addr = if self.sdhc { lba as u32 } else { (lba * BLOCK_SIZE as u64) as u32 };

        // Clear interrupts.
        self.write(SDHCI_INT_STATUS, 0xFFFF_FFFF);

        self.write(SDHCI_ARGUMENT, addr);

        let cmd_val = (SD_CMD_READ_SINGLE << CMD_INDEX_SHIFT)
            | CMD_RESP_48
            | CMD_CRC_CHECK
            | CMD_INDEX_CHECK
            | CMD_DATA_PRESENT
            | XFER_READ
            | XFER_BLOCK_COUNT_EN;
        self.write(SDHCI_XFER_MODE_CMD, cmd_val);

        // Wait for command complete.
        for _ in 0..PIO_TIMEOUT {
            let status = self.read(SDHCI_INT_STATUS);
            if status & INT_ERROR != 0 {
                self.write(SDHCI_INT_STATUS, status);
                if status & INT_ERR_CMD_TIMEOUT != 0 || status & INT_ERR_DATA_TIMEOUT != 0 {
                    return Err(BlockError::Timeout);
                }
                return Err(BlockError::IoError);
            }
            if status & INT_CMD_COMPLETE != 0 {
                self.write(SDHCI_INT_STATUS, INT_CMD_COMPLETE);
                break;
            }
            core::hint::spin_loop();
        }

        // Wait for buffer read ready.
        for _ in 0..PIO_TIMEOUT {
            let status = self.read(SDHCI_INT_STATUS);
            if status & INT_ERROR != 0 {
                self.write(SDHCI_INT_STATUS, status);
                return Err(BlockError::IoError);
            }
            if status & INT_BUF_READ_READY != 0 {
                self.write(SDHCI_INT_STATUS, INT_BUF_READ_READY);
                break;
            }
            core::hint::spin_loop();
        }

        // Read 512 bytes (128 × 32-bit words) from buffer data port.
        for i in 0..128 {
            let word = self.read(SDHCI_BUFFER_DATA);
            let offset = i * 4;
            buf[offset] = word as u8;
            buf[offset + 1] = (word >> 8) as u8;
            buf[offset + 2] = (word >> 16) as u8;
            buf[offset + 3] = (word >> 24) as u8;
        }

        // Wait for transfer complete.
        for _ in 0..PIO_TIMEOUT {
            let status = self.read(SDHCI_INT_STATUS);
            if status & INT_XFER_COMPLETE != 0 {
                self.write(SDHCI_INT_STATUS, INT_XFER_COMPLETE);
                break;
            }
            if status & INT_ERROR != 0 {
                self.write(SDHCI_INT_STATUS, status);
                return Err(BlockError::IoError);
            }
            core::hint::spin_loop();
        }

        Ok(())
    }

    fn write_block_pio(&self, lba: u64, buf: &[u8]) -> Result<(), BlockError> {
        self.wait_cmd_ready()?;
        self.wait_dat_ready()?;

        self.write(SDHCI_BLOCK_SIZE_COUNT, (1 << 16) | BLOCK_SIZE as u32);

        let addr = if self.sdhc { lba as u32 } else { (lba * BLOCK_SIZE as u64) as u32 };

        self.write(SDHCI_INT_STATUS, 0xFFFF_FFFF);
        self.write(SDHCI_ARGUMENT, addr);

        let cmd_val = (SD_CMD_WRITE_SINGLE << CMD_INDEX_SHIFT)
            | CMD_RESP_48
            | CMD_CRC_CHECK
            | CMD_INDEX_CHECK
            | CMD_DATA_PRESENT
            | XFER_BLOCK_COUNT_EN;
        self.write(SDHCI_XFER_MODE_CMD, cmd_val);

        // Wait for command complete.
        for _ in 0..PIO_TIMEOUT {
            let status = self.read(SDHCI_INT_STATUS);
            if status & INT_ERROR != 0 {
                self.write(SDHCI_INT_STATUS, status);
                return Err(BlockError::IoError);
            }
            if status & INT_CMD_COMPLETE != 0 {
                self.write(SDHCI_INT_STATUS, INT_CMD_COMPLETE);
                break;
            }
            core::hint::spin_loop();
        }

        // Wait for buffer write ready.
        for _ in 0..PIO_TIMEOUT {
            let status = self.read(SDHCI_INT_STATUS);
            if status & INT_ERROR != 0 {
                self.write(SDHCI_INT_STATUS, status);
                return Err(BlockError::IoError);
            }
            if status & INT_BUF_WRITE_READY != 0 {
                self.write(SDHCI_INT_STATUS, INT_BUF_WRITE_READY);
                break;
            }
            core::hint::spin_loop();
        }

        // Write 512 bytes (128 × 32-bit words) to buffer data port.
        for i in 0..128 {
            let offset = i * 4;
            let word = buf[offset] as u32
                | (buf[offset + 1] as u32) << 8
                | (buf[offset + 2] as u32) << 16
                | (buf[offset + 3] as u32) << 24;
            self.write(SDHCI_BUFFER_DATA, word);
        }

        // Wait for transfer complete.
        for _ in 0..PIO_TIMEOUT {
            let status = self.read(SDHCI_INT_STATUS);
            if status & INT_XFER_COMPLETE != 0 {
                self.write(SDHCI_INT_STATUS, INT_XFER_COMPLETE);
                break;
            }
            if status & INT_ERROR != 0 {
                self.write(SDHCI_INT_STATUS, status);
                return Err(BlockError::IoError);
            }
            core::hint::spin_loop();
        }

        Ok(())
    }
}

impl BlockDevice for Emmc2 {
    fn read_block(&mut self, lba: u64, buf: &mut [u8]) -> Result<(), BlockError> {
        if !self.initialized {
            return Err(BlockError::NoMedia);
        }
        if lba >= self.block_count {
            return Err(BlockError::InvalidLba);
        }
        if self.use_adma2 {
            self.read_block_adma2(lba, buf)
        } else {
            self.read_block_pio(lba, buf)
        }
    }

    fn write_block(&mut self, lba: u64, buf: &[u8]) -> Result<(), BlockError> {
        if !self.initialized {
            return Err(BlockError::NoMedia);
        }
        if lba >= self.block_count {
            return Err(BlockError::InvalidLba);
        }
        if self.use_adma2 {
            self.write_block_adma2(lba, buf)
        } else {
            self.write_block_pio(lba, buf)
        }
    }

    fn block_count(&self) -> u64 {
        self.block_count
    }

    fn block_size(&self) -> usize {
        BLOCK_SIZE
    }
}

// Module-level facade functions.

pub fn init(base: usize) -> Result<(), BlockError> {
    let mut emmc = Emmc2::new(base);

    // Check if the controller is present by reading the version register.
    let ver = emmc.read(SDHCI_SLOTISR_VER);
    if ver == 0 || ver == 0xFFFF_FFFF {
        return Err(BlockError::NoMedia);
    }

    emmc.controller_init()?;
    emmc.card_init()?;

    // SAFETY: Called once during single-core init before interrupts
    // drive storage access.
    unsafe { *EMMC.0.get() = Some(emmc) };
    Ok(())
}

pub fn is_initialized() -> bool {
    // SAFETY: Read-only check on initialized flag.
    unsafe {
        (*EMMC.0.get())
            .as_ref()
            .map(|e| e.initialized)
            .unwrap_or(false)
    }
}

pub fn card_info() -> Option<(bool, u64)> {
    // SAFETY: Immutable read of card parameters.
    unsafe {
        (*EMMC.0.get())
            .as_ref()
            .filter(|e| e.initialized)
            .map(|e| (e.sdhc, e.block_count))
    }
}

pub fn read_block(lba: u64, buf: &mut [u8]) -> Result<(), BlockError> {
    // SAFETY: Caller is responsible for serializing access (e.g., via spinlock).
    unsafe {
        (*EMMC.0.get())
            .as_mut()
            .ok_or(BlockError::NoMedia)?
            .read_block(lba, buf)
    }
}

pub fn write_block(lba: u64, buf: &[u8]) -> Result<(), BlockError> {
    // SAFETY: Caller is responsible for serializing access.
    unsafe {
        (*EMMC.0.get())
            .as_mut()
            .ok_or(BlockError::NoMedia)?
            .write_block(lba, buf)
    }
}
