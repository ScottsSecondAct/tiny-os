// RP1 Ethernet MAC — Synopsys DesignWare GMAC skeleton driver.
//
// MAC Registers (from RP1_ETH_BASE):
//   0x0000  MAC_CONFIG        (TE, RE, DM, FES, PS, JE, JD)
//   0x0004  MAC_FRAME_FILTER  (PR, HUC, HMC, DAIF, PM)
//   0x0010  MAC_GMII_ADDR     (PA, GR, GW, CR, GB)
//   0x0014  MAC_GMII_DATA     (GD)
//   0x0040  MAC_ADDR0_HIGH    (ADDRHI, MO, AE)
//   0x0044  MAC_ADDR0_LOW     (ADDRLO)
//
// DMA Registers:
//   0x1000  DMA_BUS_MODE      (SWR, PBL, DSL, DA, ATDS)
//   0x100C  DMA_RX_BASE_ADDR  Receive descriptor list base
//   0x1010  DMA_TX_BASE_ADDR  Transmit descriptor list base
//   0x1014  DMA_STATUS         (TI, TPS, TU, RI, RU, NIS, AIS)
//   0x1018  DMA_OP_MODE       (SR, ST, TSF, RSF)

use super::memory_map::RP1_ETH_BASE;
use arch::net::{NetDevice, NetError};

#[allow(dead_code)]
const MAC_CONFIG: usize = 0x0000;
#[allow(dead_code)]
const MAC_FRAME_FILTER: usize = 0x0004;
#[allow(dead_code)]
const MAC_GMII_ADDR: usize = 0x0010;
#[allow(dead_code)]
const MAC_GMII_DATA: usize = 0x0014;
#[allow(dead_code)]
const MAC_ADDR0_HIGH: usize = 0x0040;
#[allow(dead_code)]
const MAC_ADDR0_LOW: usize = 0x0044;
#[allow(dead_code)]
const DMA_BUS_MODE: usize = 0x1000;
#[allow(dead_code)]
const DMA_RX_BASE_ADDR: usize = 0x100C;
#[allow(dead_code)]
const DMA_TX_BASE_ADDR: usize = 0x1010;
#[allow(dead_code)]
const DMA_STATUS: usize = 0x1014;
#[allow(dead_code)]
const DMA_OP_MODE: usize = 0x1018;

pub struct Rp1Eth;

impl Rp1Eth {
    pub const fn new() -> Self {
        Self
    }

    #[inline(always)]
    #[allow(dead_code)]
    fn reg(offset: usize) -> *mut u32 {
        (RP1_ETH_BASE + offset) as *mut u32
    }

    #[inline(always)]
    #[allow(dead_code)]
    fn read(offset: usize) -> u32 {
        // SAFETY: RP1_ETH_BASE is a valid MMIO address within the RP1
        // peripheral window, which is identity-mapped.
        unsafe { core::ptr::read_volatile(Self::reg(offset)) }
    }

    #[inline(always)]
    #[allow(dead_code)]
    fn write(offset: usize, val: u32) {
        // SAFETY: Same as read — valid MMIO, volatile prevents reordering.
        unsafe { core::ptr::write_volatile(Self::reg(offset), val) }
    }
}

impl NetDevice for Rp1Eth {
    fn send(&mut self, _buf_idx: u16) -> Result<(), NetError> {
        Err(NetError::NoLink)
    }

    fn recv(&mut self) -> Option<u16> {
        None
    }

    fn mac_addr(&self) -> [u8; 6] {
        [0; 6]
    }

    fn has_link(&self) -> bool {
        false
    }
}
