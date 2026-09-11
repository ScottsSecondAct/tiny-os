# Machine Vision Inspector

User-space EL0 application that simulates an industrial machine vision quality inspection pipeline. Demonstrates a realistic multi-stage image processing workflow running entirely within the tiny_os RTOS syscall framework.

## What It Demonstrates

- **Industrial AOI pipeline**: frame acquisition, threshold, blob detection, feature extraction, classification, result logging, and network alerting
- **GPIO interaction**: trigger (pin 5) and illumination strobe (pin 6) for camera capture sequencing
- **Filesystem logging**: each inspection result written to `/VISION.TXT` via FS syscalls
- **Network alerts**: UDP packets sent to monitoring port 6000 on reject
- **Graceful fallback**: runs in simulation mode when GPIO returns `E_PERM` (QEMU)
- **Deterministic defect patterns**: reproducible pass/fail distribution for testing
- **Temperature integration**: reads SoC temperature via `SYS_TEMPERATURE` to seed frame variation and log ambient conditions

## Processing Pipeline

The app executes a 7-stage pipeline for each inspection cycle (target: 2 inspections per second):

1. **Frame Acquisition** — Fire GPIO strobe and trigger (or skip in simulation mode). Generate a synthetic 32x32 grayscale frame with procedural patterns.

2. **Threshold** — Convert grayscale to binary in-place (pixel > 128 = foreground).

3. **Blob Detection** — 4-connectivity connected component counting: a foreground pixel with no left or above foreground neighbor starts a new blob. Exact for the rectangular patterns used.

4. **Feature Extraction** — Compute area (total foreground pixels), centroid (average x/y of foreground pixels), and blob count.

5. **Classification** — Determine pass/fail with confidence score (see rules below).

6. **Result Logging** — Write result to `/VISION.TXT` via FS syscalls (`FS_CREATE` / `FS_WRITE` / `FS_CLOSE`). On failure, send UDP alert with inspection number and reason.

7. **Statistics Dashboard** — Every 10 inspections, print summary: total inspected, pass/fail counts, yield percentage, average processing time.

## Classification Rules

| Result | Criteria |
|--------|----------|
| **PASS** | Exactly 1 blob, area in [200, 800] pixels, centroid within 4 pixels of frame center (16, 16) |
| **FAIL: no_object** | Area is 0 (nothing detected) |
| **FAIL: multiple** | More than 1 blob (multiple disconnected objects) |
| **FAIL: undersized** | Area < 200 pixels (part too small or missing material) |
| **FAIL: oversized** | Area > 800 pixels (excess material or wrong part) |
| **FAIL: off_center** | Centroid more than 4 pixels from center (misaligned placement) |

Confidence score (PASS only, 0-100): starts at 100, penalized by area deviation from ideal (500 pixels) and centroid distance from center.

## Simulated Defect Patterns

Defects are injected deterministically based on the inspection counter:

| Pattern | Trigger | Frame Content | Area | Blobs | Result |
|---------|---------|---------------|------|-------|--------|
| Normal | Default | 20x20 centered rectangle | ~400 | 1 | PASS (score 78-90) |
| Two blobs | Every 7th frame | Two 10x12 rectangles | ~240 | 2 | FAIL: multiple |
| Oversized | Every 13th frame | 30x30 rectangle | ~900 | 1 | FAIL: oversized |
| Undersized | Every 19th frame | 8x8 rectangle | ~64 | 1 | FAIL: undersized |
| Off-center | Every 23rd frame | 20x20 rectangle in corner | ~400 | 1 | FAIL: off_center |

Normal frames have slight position variation (1-2 pixel offset) seeded by the SoC temperature reading at boot, so the exact confidence scores vary between runs.

## Syscalls Used

| Syscall | Purpose |
|---------|---------|
| `SYS_YIELD` (0) | Yield CPU when processing exceeds cycle budget |
| `SYS_DELAY` (1) | Pace inspection cycle to 500ms, GPIO exposure timing |
| `SYS_WRITE` (2) | Print formatted results and statistics to UART console |
| `SYS_UPTIME` (4) | Measure per-inspection processing time |
| `SYS_TEMPERATURE` (6) | Read SoC temperature for frame seed and result logging |
| `SYS_FS` (10) | Create, write, and close log file (`/VISION.TXT`) |
| `SYS_NET` (11) | Open UDP socket and send reject alert packets |
| `SYS_GPIO` (14) | Configure trigger/strobe pins, fire capture sequence |

## Sample Output

```
[vision] machine vision inspector (EL0)
[vision] init trigger/strobe.. simulated
[vision] GPIO unavailable, simulation mode
[vision] init alert socket.. ok
[vision] pipeline running
[vision] [00001] PASS score=84 area=400 blobs=1 cx=15 cy=14 t=0
[vision] [00002] PASS score=87 area=400 blobs=1 cx=16 cy=14 t=0
[vision] [00003] PASS score=84 area=400 blobs=1 cx=17 cy=14 t=0
[vision] [00004] PASS score=84 area=400 blobs=1 cx=14 cy=15 t=0
[vision] [00005] PASS score=87 area=400 blobs=1 cx=15 cy=15 t=0
[vision] [00006] PASS score=90 area=400 blobs=1 cx=16 cy=15 t=0
[vision] [00007] FAIL:multiple area=240 blobs=2 cx=16 cy=16 t=0
[vision] [00008] PASS score=84 area=400 blobs=1 cx=17 cy=15 t=0
[vision] [00009] PASS score=84 area=400 blobs=1 cx=14 cy=16 t=0
[vision] [00010] PASS score=87 area=400 blobs=1 cx=15 cy=16 t=0
[vision] inspected=10 pass=9 fail=1 yield=90% avg_time=0ms
[vision] [00011] PASS score=90 area=400 blobs=1 cx=16 cy=16 t=0
[vision] [00012] PASS score=84 area=400 blobs=1 cx=17 cy=16 t=0
[vision] [00013] FAIL:oversized area=900 blobs=1 cx=15 cy=15 t=0
[vision] [00014] FAIL:multiple area=240 blobs=2 cx=16 cy=16 t=0
```

On real Pi 5 hardware, the `t=` field shows the SoC temperature in degrees Celsius (e.g., `t=45`). On QEMU, `SYS_TEMPERATURE` is unavailable and `t=0` is shown.

## Design

- All code and data placed in `.user.text` linker section for EL0 accessibility
- No heap allocation; all formatting uses fixed-size stack buffers with volatile read/write
- Frame buffer (1024 bytes) and output buffer (128 bytes) live on the user stack (~1.2 KB total)
- GPIO capability checked at startup; entire pipeline runs without hardware GPIO
- Per-inspection FS create/write/close cycle (matches sensor_gateway pattern)
- UDP socket opened once at startup, reused for all reject alerts
- 4-connectivity blob detection: O(n) single-pass over the frame, exact for axis-aligned rectangles

## Real-World Relevance

This example models the core loop found in real industrial vision systems:

- **Automated Optical Inspection (AOI)**: PCB solder joint inspection, component placement verification
- **Pick-and-place verification**: confirming correct part placement before assembly proceeds
- **Dimensional measurement**: checking that parts meet size tolerances
- **Surface defect detection**: scratches, dents, contamination on manufactured surfaces
- **Quality control**: pass/fail gating with configurable acceptance criteria and reject alerting
- **Food/pharma packaging**: fill level verification, label presence, seal integrity

In a production system, the synthetic frame generator would be replaced with a real camera driver (e.g., MIPI CSI-2 via the Pi 5's camera interface), and the blob detection could be extended to flood-fill or union-find for arbitrary shapes.
