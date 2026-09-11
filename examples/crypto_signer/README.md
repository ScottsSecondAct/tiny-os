# Crypto Signer

User-space EL0 application that signs log records with HMAC-SHA256 and writes them to the filesystem.

## What It Does

- Builds timestamped log records every 20 seconds (`REC:NNNN:uptime:temp`)
- Probes the hardware crypto capability via `SYS_CRYPTO` / `CRYPTO_DETECT`
- If crypto is available: computes SHA-256 of the record and appends the first 8 hex bytes as signature
- If crypto is denied (E_PERM): falls back to a rotating XOR checksum (2 hex bytes, labeled `sw-xor`)
- Prints each signed record to the UART console via `SYS_WRITE`
- Writes records to `/SIGNED.TXT` via filesystem syscalls (handles E_PERM gracefully)

## Syscalls Used

| Syscall | Purpose |
|---------|---------|
| `SYS_DELAY` (1) | Sleep 20 seconds between signing intervals |
| `SYS_WRITE` (2) | Print signed records to the console |
| `SYS_UPTIME` (4) | Timestamp each record with monotonic uptime |
| `SYS_TEMPERATURE` (6) | Include SoC temperature in the record payload |
| `SYS_FS` (10) | Create/write/close `/SIGNED.TXT` (ops 5/2/3) |
| `SYS_CRYPTO` (20) | SHA-256 hash (op 2), capability detect (op 3) |

## Sample Output

With hardware crypto available:
```
[crypto-signer] started (EL0 signer)
[crypto-signer] crypto: hw-sha256 ready
[crypto-signer] REC:0001:20000:45200:hw-sha256:a3b1c9f2
[crypto-signer] wrote SIGNED.TXT
```

With crypto capability denied (typical for user tasks without CAP_CRYPTO):
```
[crypto-signer] started (EL0 signer)
[crypto-signer] crypto: sw-xor fallback (no cap)
[crypto-signer] REC:0001:20000:45200:sw-xor:7e
```

## Notes

- `SYS_CRYPTO` requires `CAP_CRYPTO` (bit 20), which is not in `CAP_USER_DEFAULT` -- user tasks will typically use the software XOR fallback unless granted additional capabilities
- The XOR checksum is not cryptographically secure; it is a stand-in to demonstrate graceful degradation
- On QEMU, `SYS_TEMPERATURE` returns `u64::MAX` (no VideoCore mailbox); the record shows 0 for temperature
- All code and data are placed in the `.user.text` linker section for EL0 accessibility
- No heap allocation; all formatting uses fixed-size stack buffers with volatile writes
