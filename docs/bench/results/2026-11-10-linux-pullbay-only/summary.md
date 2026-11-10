# Benchmark summary

- Started: 2026-11-10T10:40:00Z on Linux-7.0.0-43-generic-x86_64-with-glibc2.43
- CPU: AMD Ryzen 7 9800X3D 8-Core Processor, 16 logical; RAM: 30.0 GiB
- File: 64 MiB, 5 runs per tool and scenario
- Tools: pullbay = pullbay-cli 0.4.0 (repository at 7006e77)
- Note: Sample run: Pullbay only, no IDM and no other manager. Release build of commit 7006e77 on Linux, against the local fault server on loopback. It shows the harness and the faults working; it is not a comparison.

Median of the runs that got the whole, correct file. CPU is user plus system time. Connections is the most the server saw at once; sent is the bytes it sent over the file's size.

| Scenario | Tool | OK | Median s | Min s | Max s | MiB/s | CPU s | Connections | Sent |
|---|---|---|---|---|---|---|---|---|---|
| clean | pullbay | 5/5 | 0.17 | 0.17 | 0.17 | 383.2 | 0.14 | 8 | 1.10x |
| per-connection-limit | pullbay | 5/5 | 2.12 | 2.12 | 2.12 | 30.2 | 0.07 | 8 | 1.00x |
| slow-link | pullbay | 5/5 | 4.13 | 4.13 | 4.13 | 15.5 | 0.10 | 8 | 1.00x |
| drops | pullbay | 5/5 | 1.53 | 1.52 | 1.53 | 41.9 | 0.06 | 8 | 1.00x |
| two-per-client | pullbay | 5/5 | 8.12 | 8.12 | 8.12 | 7.9 | 0.10 | 2 | 1.00x |
| slow-start | pullbay | 5/5 | 4.12 | 4.12 | 4.12 | 15.5 | 0.07 | 8 | 1.00x |
| wrong-ranges | pullbay | 5/5 | 3.37 | 3.37 | 3.37 | 19.0 | 0.09 | 8 | 1.01x |
| no-ranges | pullbay | 5/5 | 4.13 | 4.13 | 4.13 | 15.5 | 0.07 | 2 | 1.00x |
