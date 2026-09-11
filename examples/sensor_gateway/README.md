# Industrial Sensor Gateway

User-space EL0 application that collects sensor data from SPI, I2C, and GPIO peripherals, logs readings to the SD card filesystem, and forwards telemetry over UDP.

## What It Does

- Initializes SPI bus (pressure sensor, e.g., BMP280 in SPI mode)
- Initializes I2C bus (temperature/humidity sensor, e.g., BMP280 at 0x76)
- Configures GPIO pin 17 as input (digital vibration switch)
- Opens a UDP socket for telemetry forwarding
- Samples all sensors every 1 second:
  - **Temperature** via I2C (falls back to SoC temperature if I2C unavailable)
  - **Pressure** via SPI (falls back to 1013.25 hPa standard atmosphere)
  - **Vibration** via GPIO digital input
- Logs each reading to `sensor.log` on the FAT32 filesystem
- Forwards each reading over UDP to the monitoring host
- Prints a summary to the UART console

## Syscalls Used

| Syscall | Purpose |
|---------|---------|
| `SYS_DELAY` (1) | Sleep between sampling intervals |
| `SYS_WRITE` (2) | Print formatted output to the console |
| `SYS_UPTIME` (4) | Timestamp each reading |
| `SYS_TEMPERATURE` (6) | Fallback SoC temperature reading |
| `SYS_FS` (10) | Create/write/close log file on SD card |
| `SYS_NET` (11) | Open UDP socket and send telemetry |
| `SYS_SPI` (12) | Configure SPI bus and transfer data |
| `SYS_I2C` (13) | Configure I2C bus, write register address, read sensor data |
| `SYS_GPIO` (14) | Configure GPIO pin as input and read state |

## Sample Output

```
[gateway] industrial sensor gateway (EL0)
[gateway] init SPI bus.. stub (no hw)
[gateway] init I2C bus.. stub (no hw)
[gateway] init GPIO..    stub (no hw)
[gateway] init network.. ok
[gateway] sampling started
[gateway] 1 T=45200mC P=101325hPa  V=0
[gateway] 2 T=45300mC P=101325hPa  V=0
```

## Design

- Runs at priority 80 (`MissionCritical`) for deterministic sensor sampling
- On QEMU (no RP1 hardware), SPI/I2C/GPIO return `E_NOSYS`; the app gracefully falls back to SoC temperature and default values
- On real Pi 5 hardware, the RP1 SPI/I2C/GPIO drivers provide actual peripheral access
- Log format: `t=<tick>,T=<temp_mc>,P=<pressure_hpa>,V=<0|1>`
- UDP telemetry format: `T=<temp> P=<pressure> V=<vibration>`

## Scheduling Context

This application demonstrates the RTOS scheduling model for industrial use:

| Task | Priority | Criticality | Role |
|------|----------|-------------|------|
| `sensor-gw` | 80 | MissionCritical | Sensor sampling (this app) |
| `temp-mon` | 100 | Standard | SoC temperature monitoring |
| `shell` | 200 | Standard | Diagnostics interface |

Higher priority numbers = lower priority. The sensor gateway (80) preempts the shell (200) but yields to safety-critical kernel tasks.
